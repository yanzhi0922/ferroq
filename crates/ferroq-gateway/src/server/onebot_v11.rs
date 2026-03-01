//! OneBot v11 inbound protocol server.
//!
//! Exposes three interfaces to upstream bot frameworks:
//!
//! 1. **HTTP API** — `POST /onebot/v11/api/:action` for API calls
//! 2. **Forward WebSocket** — `ws://host:port/onebot/v11/ws` for bidirectional
//! 3. **Reverse WebSocket** — connects to configured targets and pushes events
//!
//! Also supports HTTP POST event reporting to configured URLs.

use std::sync::Arc;

use axum::extract::ws::{Message as AxumWsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{any, post};
use axum::Json;
use ferroq_core::api::{ApiRequest, ApiResponse};
use ferroq_core::config::{OneBotV11Config, WsReverseTarget};
use ferroq_core::event::Event;
use futures::{SinkExt, StreamExt};
use parking_lot::Mutex;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::onebot_v11 as parser;
use crate::router::ApiRouter;

/// Shared state for the axum handlers.
#[allow(dead_code)]
struct ServerState {
    router: Arc<ApiRouter>,
    bus_tx: broadcast::Sender<Event>,
    access_token: String,
}

/// OneBot v11 inbound protocol server.
pub struct OneBotV11Server {
    config: OneBotV11Config,
    access_token: String,
    /// Handles for reverse WS tasks.
    reverse_ws_handles: Mutex<Vec<JoinHandle<()>>>,
    /// Handles for HTTP POST reporting tasks.
    http_post_handles: Mutex<Vec<JoinHandle<()>>>,
}

impl OneBotV11Server {
    /// Create a new OneBot v11 server from config.
    pub fn new(config: OneBotV11Config, access_token: String) -> Self {
        Self {
            config,
            access_token,
            reverse_ws_handles: Mutex::new(Vec::new()),
            http_post_handles: Mutex::new(Vec::new()),
        }
    }

    /// Build the axum Router for this protocol server.
    ///
    /// The caller should nest this under `/onebot/v11` or similar.
    pub fn build_router(
        &self,
        router: Arc<ApiRouter>,
        bus_tx: broadcast::Sender<Event>,
    ) -> axum::Router {
        let state = Arc::new(ServerState {
            router,
            bus_tx,
            access_token: self.access_token.clone(),
        });

        let mut app = axum::Router::new();

        if self.config.http {
            app = app.route("/api/{action}", post(handle_http_api));
            // Also support the legacy `/action` path.
            app = app.route("/api", post(handle_http_api_legacy));
        }

        if self.config.ws {
            app = app.route("/ws", any(handle_ws_upgrade));
        }

        app.with_state(state)
    }

    /// Start reverse WebSocket connections and HTTP POST reporting.
    ///
    /// Call this after the event bus is ready. These run as background tasks.
    pub fn start_background_tasks(
        &self,
        router: Arc<ApiRouter>,
        bus_tx: broadcast::Sender<Event>,
    ) {
        // Reverse WebSocket targets.
        for target in &self.config.ws_reverse {
            let handle = tokio::spawn(reverse_ws_task(
                target.clone(),
                Arc::clone(&router),
                bus_tx.subscribe(),
                self.access_token.clone(),
            ));
            self.reverse_ws_handles.lock().push(handle);
        }

        // HTTP POST targets.
        for target in &self.config.http_post {
            let handle = tokio::spawn(http_post_task(
                target.url.clone(),
                target.secret.clone(),
                bus_tx.subscribe(),
            ));
            self.http_post_handles.lock().push(handle);
        }
    }

    /// Stop all background tasks.
    pub fn stop_background_tasks(&self) {
        for handle in self.reverse_ws_handles.lock().drain(..) {
            handle.abort();
        }
        for handle in self.http_post_handles.lock().drain(..) {
            handle.abort();
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP API handler
// ---------------------------------------------------------------------------

/// Handle HTTP API requests: `POST /api/{action}`
async fn handle_http_api(
    State(state): State<Arc<ServerState>>,
    Path(action): Path<String>,
    body: String,
) -> impl IntoResponse {
    // Auth check.
    // (For simplicity, we skip auth check on HTTP for now — it should
    //  be done via middleware with the Authorization header.)

    let params: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();

    let request = ApiRequest {
        action,
        params,
        echo: None,
        self_id: None,
    };

    match state.router.route(request).await {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => {
            let resp = ApiResponse::fail(1400, e.to_string());
            Json(resp).into_response()
        }
    }
}

/// Handle legacy HTTP API: `POST /api` with action in body.
async fn handle_http_api_legacy(
    State(state): State<Arc<ServerState>>,
    body: String,
) -> impl IntoResponse {
    let request: ApiRequest = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            let resp = ApiResponse::fail(1400, format!("invalid request: {e}"));
            return Json(resp).into_response();
        }
    };

    match state.router.route(request).await {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => {
            let resp = ApiResponse::fail(1400, e.to_string());
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
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_connection(socket, state))
}

/// Handle an active forward WebSocket connection.
async fn handle_ws_connection(socket: WebSocket, state: Arc<ServerState>) {
    let (ws_tx, mut ws_rx) = socket.split();
    let mut event_rx = state.bus_tx.subscribe();
    let router = Arc::clone(&state.router);

    info!("new OneBot v11 forward WS client connected");

    // Shared writer channel — both event push and API responses write through this.
    let (msg_tx, mut msg_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // Task: drain msg_rx and write to WS.
    let ws_tx = Arc::new(tokio::sync::Mutex::new(ws_tx));
    let ws_tx_clone = Arc::clone(&ws_tx);
    let writer_task = tokio::spawn(async move {
        while let Some(text) = msg_rx.recv().await {
            let mut tx = ws_tx_clone.lock().await;
            if tx.send(AxumWsMessage::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    // Task: push events to the WS client.
    let msg_tx_events = msg_tx.clone();
    let push_task = tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    let json = parser::event_to_json(&event);
                    let text = serde_json::to_string(&json).unwrap_or_default();
                    if msg_tx_events.send(text).is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "forward WS client lagged, skipping events");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Receive API requests from the WS client.
    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            AxumWsMessage::Text(text) => {
                let request: ApiRequest = match serde_json::from_str(&text) {
                    Ok(r) => r,
                    Err(e) => {
                        debug!("invalid WS API request: {e}");
                        continue;
                    }
                };

                let echo = request.echo.clone();
                let msg_tx_resp = msg_tx.clone();
                let router_clone = Arc::clone(&router);

                // Process API call and send response back on the same WS.
                tokio::spawn(async move {
                    let resp = match router_clone.route(request).await {
                        Ok(mut r) => {
                            r.echo = echo;
                            r
                        }
                        Err(e) => ApiResponse::fail(1400, e.to_string()).with_echo(echo),
                    };
                    let text = serde_json::to_string(&resp).unwrap_or_default();
                    let _ = msg_tx_resp.send(text);
                });
            }
            AxumWsMessage::Close(_) => break,
            _ => {}
        }
    }

    push_task.abort();
    writer_task.abort();
    info!("OneBot v11 forward WS client disconnected");
}

// ---------------------------------------------------------------------------
// Reverse WebSocket
// ---------------------------------------------------------------------------

/// Manage a reverse WebSocket connection to a target.
async fn reverse_ws_task(
    target: WsReverseTarget,
    router: Arc<ApiRouter>,
    mut event_rx: broadcast::Receiver<Event>,
    _access_token: String,
) {
    info!(url = %target.url, "starting reverse WS connection");

    loop {
        match tokio_tungstenite::connect_async(&target.url).await {
            Ok((ws_stream, _)) => {
                info!(url = %target.url, "reverse WS connected");
                let (mut ws_tx, mut ws_rx) = ws_stream.split();

                // Push events.
                let push_loop = async {
                    loop {
                        match event_rx.recv().await {
                            Ok(event) => {
                                let json = parser::event_to_json(&event);
                                let text = serde_json::to_string(&json).unwrap_or_default();
                                if ws_tx
                                    .send(tokio_tungstenite::tungstenite::Message::Text(
                                        text.into(),
                                    ))
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                warn!(skipped = n, "reverse WS lagged");
                            }
                            Err(broadcast::error::RecvError::Closed) => return,
                        }
                    }
                };

                // Receive API requests.
                let recv_loop = async {
                    while let Some(Ok(msg)) = ws_rx.next().await {
                        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                            if let Ok(request) = serde_json::from_str::<ApiRequest>(&text) {
                                let echo = request.echo.clone();
                                match router.route(request).await {
                                    Ok(mut resp) => {
                                        resp.echo = echo;
                                        debug!("API response (reverse WS): completed");
                                    }
                                    Err(e) => {
                                        debug!("API error (reverse WS): {e}");
                                    }
                                }
                            }
                        }
                    }
                };

                tokio::select! {
                    _ = push_loop => {},
                    _ = recv_loop => {},
                }

                warn!(url = %target.url, "reverse WS disconnected, reconnecting in 5s");
            }
            Err(e) => {
                error!(url = %target.url, error = %e, "reverse WS connect failed, retrying in 5s");
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

// ---------------------------------------------------------------------------
// HTTP POST event reporting
// ---------------------------------------------------------------------------

/// Push events via HTTP POST to a configured target.
async fn http_post_task(
    url: String,
    _secret: String,
    mut event_rx: broadcast::Receiver<Event>,
) {
    info!(url = %url, "starting HTTP POST event reporting");
    let client = reqwest::Client::new();

    loop {
        match event_rx.recv().await {
            Ok(event) => {
                let json = parser::event_to_json(&event);
                // TODO: HMAC signing with secret
                match client.post(&url).json(&json).send().await {
                    Ok(resp) => {
                        debug!(url = %url, status = %resp.status(), "HTTP POST event sent");
                    }
                    Err(e) => {
                        warn!(url = %url, error = %e, "HTTP POST event failed");
                    }
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!(url = %url, skipped = n, "HTTP POST lagged");
            }
            Err(broadcast::error::RecvError::Closed) => {
                info!(url = %url, "HTTP POST event reporting stopped (bus closed)");
                return;
            }
        }
    }
}
