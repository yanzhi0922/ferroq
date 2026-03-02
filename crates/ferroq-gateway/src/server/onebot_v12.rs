//! OneBot v12 inbound protocol server.
//!
//! Exposes two interfaces to upstream bot frameworks:
//!
//! 1. **HTTP API** — `POST /onebot/v12/action` for API calls
//! 2. **Forward WebSocket** — `ws://host:port/onebot/v12/ws` for bidirectional
//!
//! OneBot v12 spec: <https://12.onebot.dev/>

use std::sync::Arc;

use axum::Json;
use axum::extract::ws::{Message as AxumWsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{any, post};
use ferroq_core::config::OneBotV12Config;
use ferroq_core::event::Event;
use futures::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use crate::onebot_v12 as converter;
use crate::router::ApiRouter;
use crate::shared_config::SharedConfig;
use crate::stats::RuntimeStats;

/// Shared state for the axum handlers.
struct ServerState {
    router: Arc<ApiRouter>,
    bus_tx: broadcast::Sender<Event>,
    shared_config: Arc<SharedConfig>,
    stats: Arc<RuntimeStats>,
}

/// OneBot v12 inbound protocol server.
pub struct OneBotV12Server {
    config: OneBotV12Config,
    shared_config: Arc<SharedConfig>,
}

impl OneBotV12Server {
    /// Create a new OneBot v12 server from config.
    pub fn new(config: OneBotV12Config, shared_config: Arc<SharedConfig>) -> Self {
        Self {
            config,
            shared_config,
        }
    }

    /// Build the axum Router for this protocol server.
    ///
    /// The caller should nest this under `/onebot/v12` or similar.
    pub fn build_router(
        &self,
        router: Arc<ApiRouter>,
        bus_tx: broadcast::Sender<Event>,
        stats: Arc<RuntimeStats>,
    ) -> axum::Router {
        let state = Arc::new(ServerState {
            router,
            bus_tx,
            shared_config: Arc::clone(&self.shared_config),
            stats,
        });

        let mut app = axum::Router::new();

        if self.config.http {
            app = app.route("/action", post(handle_http_action));
            // Also support /{action} path for convenience.
            app = app.route("/action/{action}", post(handle_http_action_named));
        }

        if self.config.ws {
            app = app.route("/ws", any(handle_ws_upgrade));
        }

        app.with_state(state)
    }
}

// ---------------------------------------------------------------------------
// Access token authentication
// ---------------------------------------------------------------------------

/// Query parameter for access token.
#[derive(Debug, serde::Deserialize, Default)]
struct AuthQuery {
    #[serde(default)]
    access_token: Option<String>,
}

/// Check the access token from the Authorization header or query param.
fn check_access_token(
    state: &ServerState,
    headers: &axum::http::HeaderMap,
    query: &AuthQuery,
) -> Result<(), (StatusCode, String)> {
    let token = state.shared_config.access_token();
    if token.is_empty() {
        return Ok(());
    }

    // Check Authorization header: "Bearer <token>".
    if let Some(auth) = headers.get("Authorization").and_then(|v| v.to_str().ok()) {
        if let Some(t) = auth.strip_prefix("Bearer ") {
            if t == token {
                return Ok(());
            }
        }
    }

    // Check query parameter.
    if let Some(ref t) = query.access_token {
        if *t == token {
            return Ok(());
        }
    }

    Err((
        StatusCode::UNAUTHORIZED,
        "access token mismatch or missing".to_string(),
    ))
}

// ---------------------------------------------------------------------------
// HTTP API handlers
// ---------------------------------------------------------------------------

/// Handle HTTP action requests: `POST /action`
///
/// The request body contains the full action object per v12 spec:
/// `{ "action": "send_message", "params": {...}, "echo": "...", "self": {...} }`
async fn handle_http_action(
    State(state): State<Arc<ServerState>>,
    Query(query): Query<AuthQuery>,
    headers: axum::http::header::HeaderMap,
    body: String,
) -> impl IntoResponse {
    if let Err(resp) = check_access_token(&state, &headers, &query) {
        return resp.into_response();
    }

    let raw: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            let resp = v12_error_response(10001, format!("invalid request: {e}"), None);
            return Json(resp).into_response();
        }
    };

    let original_action = raw
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let echo = raw.get("echo").cloned();

    let request = match converter::parse_v12_action(raw) {
        Ok(r) => r,
        Err(e) => {
            let resp = v12_error_response(10001, e.to_string(), echo);
            return Json(resp).into_response();
        }
    };

    match state.router.route_named(request).await {
        Ok((resp, adapter_name)) => {
            state.stats.record_api_call_for(&adapter_name);
            let v12_resp = converter::translate_v11_response(&original_action, resp);
            Json(v12_resp).into_response()
        }
        Err(e) => {
            let resp = v12_error_response(20002, e.to_string(), echo);
            Json(resp).into_response()
        }
    }
}

/// Handle HTTP action requests with named path: `POST /action/{action}`
async fn handle_http_action_named(
    State(state): State<Arc<ServerState>>,
    axum::extract::Path(action): axum::extract::Path<String>,
    Query(query): Query<AuthQuery>,
    headers: axum::http::header::HeaderMap,
    body: String,
) -> impl IntoResponse {
    if let Err(resp) = check_access_token(&state, &headers, &query) {
        return resp.into_response();
    }

    let params: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    let echo = params.get("echo").cloned();

    // Build a v12 action object and translate through the normal path.
    let raw = serde_json::json!({
        "action": action,
        "params": params,
    });

    let request = match converter::parse_v12_action(raw) {
        Ok(r) => r,
        Err(e) => {
            let resp = v12_error_response(10001, e.to_string(), echo);
            return Json(resp).into_response();
        }
    };

    match state.router.route_named(request).await {
        Ok((resp, adapter_name)) => {
            state.stats.record_api_call_for(&adapter_name);
            let v12_resp = converter::translate_v11_response(&action, resp);
            Json(v12_resp).into_response()
        }
        Err(e) => {
            let resp = v12_error_response(20002, e.to_string(), echo);
            Json(resp).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Forward WebSocket handler
// ---------------------------------------------------------------------------

/// Handle a forward WebSocket upgrade: `GET /ws`
async fn handle_ws_upgrade(
    State(state): State<Arc<ServerState>>,
    Query(query): Query<AuthQuery>,
    headers: axum::http::header::HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if let Err(resp) = check_access_token(&state, &headers, &query) {
        return resp.into_response();
    }
    ws.on_upgrade(move |socket| handle_ws_connection(socket, state))
        .into_response()
}

/// Handle an active forward WebSocket connection.
async fn handle_ws_connection(socket: WebSocket, state: Arc<ServerState>) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let mut event_rx = state.bus_tx.subscribe();
    let router = Arc::clone(&state.router);
    let stats = Arc::clone(&state.stats);

    stats.ws_connect();
    info!("new OneBot v12 forward WS client connected");

    // Shared writer channel.
    let (msg_tx, mut msg_rx) =
        tokio::sync::mpsc::channel::<String>(crate::tuning::ws_outbound_queue_capacity());
    let api_semaphore = Arc::new(tokio::sync::Semaphore::new(
        crate::tuning::ws_api_max_in_flight(),
    ));

    // Task: drain msg_rx and write to WS.
    let writer_task = tokio::spawn(async move {
        while let Some(text) = msg_rx.recv().await {
            if ws_tx.send(AxumWsMessage::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    // Task: push events to the WS client in v12 format.
    let msg_tx_events = msg_tx.clone();
    let stats_push = Arc::clone(&stats);
    let push_task = tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    let json = converter::event_to_json(&event);
                    let text = serde_json::to_string(&json).unwrap_or_default();
                    match msg_tx_events.try_send(text) {
                        Ok(()) => {}
                        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                            stats_push.record_ws_event_dropped();
                            debug!("v12 forward WS outbound queue full, dropping event");
                        }
                        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => break,
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "v12 forward WS client lagged, skipping events");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Receive API actions from the WS client.
    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            AxumWsMessage::Text(text) => {
                let raw: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(e) => {
                        debug!("invalid WS v12 action: {e}");
                        continue;
                    }
                };

                let original_action = raw
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let echo = raw.get("echo").cloned();

                let request = match converter::parse_v12_action(raw) {
                    Ok(r) => r,
                    Err(e) => {
                        let resp = v12_error_response(10001, e.to_string(), echo);
                        let text = serde_json::to_string(&resp).unwrap_or_default();
                        let _ = msg_tx.send(text).await;
                        continue;
                    }
                };

                let msg_tx_resp = msg_tx.clone();
                let router_clone = Arc::clone(&router);
                let stats_clone = Arc::clone(&stats);
                let permit = match api_semaphore.clone().try_acquire_owned() {
                    Ok(p) => p,
                    Err(_) => {
                        let resp = v12_error_response(
                            20002,
                            "too many in-flight WS API requests".to_string(),
                            echo.clone(),
                        );
                        let text = serde_json::to_string(&resp).unwrap_or_default();
                        let _ = msg_tx.send(text).await;
                        stats.record_ws_api_rejected();
                        warn!("v12 forward WS API overload, request rejected");
                        continue;
                    }
                };

                tokio::spawn(async move {
                    let _permit = permit;
                    let resp = match router_clone.route_named(request).await {
                        Ok((resp, adapter_name)) => {
                            stats_clone.record_api_call_for(&adapter_name);
                            converter::translate_v11_response(&original_action, resp)
                        }
                        Err(e) => v12_error_response(20002, e.to_string(), echo),
                    };
                    let text = serde_json::to_string(&resp).unwrap_or_default();
                    let _ = msg_tx_resp.send(text).await;
                });
            }
            AxumWsMessage::Close(_) => break,
            _ => {}
        }
    }

    push_task.abort();
    writer_task.abort();
    stats.ws_disconnect();
    info!("OneBot v12 forward WS client disconnected");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a v12-compatible error response.
fn v12_error_response(
    retcode: i32,
    message: String,
    echo: Option<serde_json::Value>,
) -> serde_json::Value {
    serde_json::json!({
        "status": "failed",
        "retcode": retcode,
        "data": null,
        "message": message,
        "echo": echo,
    })
}
