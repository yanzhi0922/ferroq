//! Satori protocol inbound server.
//!
//! Exposes the [Satori protocol](https://satori.chat/) interfaces:
//!
//! 1. **HTTP API** — `POST /satori/v1/{resource}.{method}` (RPC-style)
//! 2. **WebSocket** — `ws://host:port/satori/v1/events` (event streaming)
//!
//! ## Authentication
//!
//! - HTTP: `Authorization: Bearer <token>` header
//! - WebSocket: token in the `IDENTIFY` signal body
//!
//! ## Required headers (HTTP)
//!
//! - `Satori-Platform`: platform name (e.g. `qq`)
//! - `Satori-User-ID`: platform account ID

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Json;
use axum::extract::State;
use axum::extract::ws::{Message as AxumWsMessage, WebSocket, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{any, post};
use ferroq_core::config::SatoriConfig;
use ferroq_core::event::Event;
use futures::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use crate::router::ApiRouter;
use crate::satori as converter;
use crate::shared_config::SharedConfig;
use crate::stats::RuntimeStats;

/// Shared state for axum handlers.
struct ServerState {
    router: Arc<ApiRouter>,
    bus_tx: broadcast::Sender<Event>,
    shared_config: Arc<SharedConfig>,
    stats: Arc<RuntimeStats>,
    /// Global sequence number counter for events.
    sn_counter: AtomicU64,
}

impl ServerState {
    fn next_sn(&self) -> u64 {
        self.sn_counter.fetch_add(1, Ordering::Relaxed) + 1
    }
}

/// Satori protocol inbound server.
pub struct SatoriServer {
    config: SatoriConfig,
    shared_config: Arc<SharedConfig>,
}

impl SatoriServer {
    /// Create a new Satori server from config.
    pub fn new(config: SatoriConfig, shared_config: Arc<SharedConfig>) -> Self {
        Self {
            config,
            shared_config,
        }
    }

    /// Build the axum Router for this protocol server.
    ///
    /// The caller should nest this under `/satori/v1` or similar.
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
            sn_counter: AtomicU64::new(0),
        });

        let mut app = axum::Router::new();

        if self.config.http {
            // Satori HTTP RPC: POST /{resource}.{method}
            app = app.route("/{resource_method}", post(handle_http_api));
        }

        if self.config.ws {
            // Satori WebSocket: /events
            app = app.route("/events", any(handle_ws_upgrade));
        }

        app.with_state(state)
    }
}

// ---------------------------------------------------------------------------
// Authentication
// ---------------------------------------------------------------------------

/// Check the access token from the Authorization header.
fn check_auth(
    state: &ServerState,
    headers: &axum::http::HeaderMap,
) -> Result<(), (StatusCode, String)> {
    let token = state.shared_config.access_token();
    if token.is_empty() {
        return Ok(());
    }

    if let Some(auth) = headers.get("Authorization").and_then(|v| v.to_str().ok()) {
        if let Some(t) = auth.strip_prefix("Bearer ") {
            if t == token {
                return Ok(());
            }
        }
    }

    Err((
        StatusCode::UNAUTHORIZED,
        serde_json::json!({"message": "unauthorized"}).to_string(),
    ))
}

/// Extract `Satori-User-ID` header as an optional i64 self_id.
fn extract_self_id(headers: &axum::http::HeaderMap) -> Option<i64> {
    headers
        .get("Satori-User-ID")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok())
}

// ---------------------------------------------------------------------------
// HTTP API handler
// ---------------------------------------------------------------------------

/// Handle Satori HTTP API: `POST /{resource}.{method}`
///
/// e.g. `POST /message.create`, `POST /login.get`
async fn handle_http_api(
    State(state): State<Arc<ServerState>>,
    axum::extract::Path(resource_method): axum::extract::Path<String>,
    headers: axum::http::header::HeaderMap,
    body: String,
) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp.into_response();
    }

    let self_id = extract_self_id(&headers);

    let body_json: serde_json::Value = if body.is_empty() {
        serde_json::Value::Object(Default::default())
    } else {
        match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    serde_json::json!({"message": format!("invalid JSON: {e}")}).to_string(),
                )
                    .into_response();
            }
        }
    };

    let request = match converter::parse_satori_api(&resource_method, body_json, self_id) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                serde_json::json!({"message": e.to_string()}).to_string(),
            )
                .into_response();
        }
    };

    match state.router.route_named(request).await {
        Ok((resp, adapter_name)) => {
            state.stats.record_api_call_for(&adapter_name);
            let data = converter::translate_response(&resource_method, resp);
            Json(data).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::json!({"message": e.to_string()}).to_string(),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// WebSocket handler
// ---------------------------------------------------------------------------

/// Handle WebSocket upgrade: `GET /events`
async fn handle_ws_upgrade(
    State(state): State<Arc<ServerState>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // Auth is done in the IDENTIFY signal, not at upgrade time.
    ws.on_upgrade(move |socket| handle_ws_connection(socket, state))
        .into_response()
}

/// Handle an active Satori WebSocket connection.
///
/// Protocol flow:
/// 1. Client sends IDENTIFY `{ "op": 3, "body": { "token": "...", "sn": N } }`
/// 2. Server replies READY `{ "op": 4, "body": { "logins": [...], "proxy_urls": [] } }`
/// 3. Client sends PING `{ "op": 1 }` every ~10s
/// 4. Server replies PONG `{ "op": 2 }`
/// 5. Server pushes EVENT `{ "op": 0, "body": { ... } }`
async fn handle_ws_connection(socket: WebSocket, state: Arc<ServerState>) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let stats = Arc::clone(&state.stats);

    stats.ws_connect();
    info!("new Satori WS client connected");

    // Wait for IDENTIFY signal (with 10 second timeout).
    let identify_result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(Ok(msg)) = ws_rx.next().await {
            if let AxumWsMessage::Text(text) = msg {
                if let Ok(signal) = serde_json::from_str::<serde_json::Value>(&text) {
                    let op = signal.get("op").and_then(|v| v.as_u64()).unwrap_or(255);
                    if op == converter::Opcode::Identify as u64 {
                        return Some(signal);
                    }
                }
            }
        }
        None
    })
    .await;

    let (identify_signal, resume_sn) = match identify_result {
        Ok(Some(signal)) => {
            let body = signal.get("body").cloned().unwrap_or_default();
            let token = body
                .get("token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let sn = body.get("sn").and_then(|v| v.as_u64()).unwrap_or(0);

            // Verify token.
            let required_token = state.shared_config.access_token();
            if !required_token.is_empty() && token != required_token {
                warn!("Satori WS client failed authentication");
                let err = serde_json::json!({
                    "op": converter::Opcode::Ready as u8,
                    "body": { "logins": [], "proxy_urls": [] },
                });
                let _ = ws_tx
                    .send(AxumWsMessage::Text(
                        serde_json::to_string(&err).unwrap_or_default().into(),
                    ))
                    .await;
                let _ = ws_tx.close().await;
                stats.ws_disconnect();
                return;
            }

            (true, sn)
        }
        _ => {
            warn!("Satori WS client did not send IDENTIFY in time");
            let _ = ws_tx.close().await;
            stats.ws_disconnect();
            return;
        }
    };

    if !identify_signal {
        stats.ws_disconnect();
        return;
    }

    // Send READY signal.
    let ready = serde_json::json!({
        "op": converter::Opcode::Ready as u8,
        "body": {
            "logins": [],
            "proxy_urls": [],
        },
    });
    {
        if ws_tx
            .send(AxumWsMessage::Text(
                serde_json::to_string(&ready).unwrap_or_default().into(),
            ))
            .await
            .is_err()
        {
            stats.ws_disconnect();
            return;
        }
    }

    info!(
        resume_sn = resume_sn,
        "Satori WS client authenticated, starting event push"
    );

    // Subscribe to events.
    let mut event_rx = state.bus_tx.subscribe();

    // Channel for sending messages (events + pong).
    let (msg_tx, mut msg_rx) =
        tokio::sync::mpsc::channel::<String>(crate::tuning::ws_outbound_queue_capacity());

    // Writer task: drain msg_rx → WS.
    let writer_task = tokio::spawn(async move {
        while let Some(text) = msg_rx.recv().await {
            if ws_tx.send(AxumWsMessage::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    // Event push task.
    let msg_tx_events = msg_tx.clone();
    let state_push = Arc::clone(&state);
    let push_task = tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    let sn = state_push.next_sn();
                    let signal = converter::event_to_signal(&event, sn);
                    let text = serde_json::to_string(&signal).unwrap_or_default();
                    match msg_tx_events.try_send(text) {
                        Ok(()) => {}
                        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                            state_push.stats.record_ws_event_dropped();
                            debug!("Satori WS outbound queue full, dropping event");
                        }
                        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => break,
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "Satori WS client lagged, skipping events");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Receive PING and API calls from the client.
    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            AxumWsMessage::Text(text) => {
                let signal: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(e) => {
                        debug!("invalid Satori WS signal: {e}");
                        continue;
                    }
                };

                let op = signal.get("op").and_then(|v| v.as_u64()).unwrap_or(255);

                match op {
                    1 => {
                        // PING → PONG
                        let pong = serde_json::json!({
                            "op": converter::Opcode::Pong as u8,
                        });
                        let text = serde_json::to_string(&pong).unwrap_or_default();
                        let _ = msg_tx.send(text).await;
                    }
                    _ => {
                        debug!(op, "unexpected Satori WS opcode");
                    }
                }
            }
            AxumWsMessage::Close(_) => break,
            _ => {}
        }
    }

    push_task.abort();
    writer_task.abort();
    stats.ws_disconnect();
    info!("Satori WS client disconnected");
}
