//! Lagrange.OneBot WebSocket backend adapter.
//!
//! Connects to a Lagrange.OneBot instance via its forward WebSocket endpoint,
//! receives OneBot v11 events, and forwards API calls.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use ferroq_core::adapter::{AdapterInfo, AdapterState, BackendAdapter};
use ferroq_core::api::{ApiRequest, ApiResponse};
use ferroq_core::error::GatewayError;
use ferroq_core::event::Event;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{connect_async, MaybeTlsStream};
use tracing::{debug, error, info, warn};

use super::super::onebot_v11;

type WsStream = tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// Internal shared mutable state.
struct Inner {
    state: AdapterState,
    self_id: Option<i64>,
    /// Channel to send WS messages to the writer task.
    ws_writer_tx: Option<mpsc::UnboundedSender<WsMessage>>,
    /// Pending API calls: echo string → response sender.
    pending_calls: HashMap<String, oneshot::Sender<ApiResponse>>,
    /// Event channel to send events to the runtime.
    event_tx: Option<mpsc::UnboundedSender<Event>>,
    /// Join handles for background tasks.
    reader_handle: Option<JoinHandle<()>>,
    writer_handle: Option<JoinHandle<()>>,
    reconnect_handle: Option<JoinHandle<()>>,
}

/// Lagrange.OneBot WebSocket backend adapter.
///
/// Connects to a Lagrange.OneBot forward WS endpoint, parses incoming
/// OneBot v11 events into internal [`Event`] types, and forwards API
/// calls as JSON over the same WebSocket.
pub struct LagrangeAdapter {
    name: String,
    url: String,
    access_token: String,
    reconnect_interval: Duration,
    #[allow(dead_code)]
    health_check_interval: Duration,
    inner: Arc<Mutex<Inner>>,
    echo_counter: AtomicU64,
}

impl LagrangeAdapter {
    /// Create a new Lagrange adapter from configuration.
    pub fn new(
        name: impl Into<String>,
        url: impl Into<String>,
        access_token: impl Into<String>,
        reconnect_interval_secs: u64,
        health_check_interval_secs: u64,
    ) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            access_token: access_token.into(),
            reconnect_interval: Duration::from_secs(reconnect_interval_secs),
            health_check_interval: Duration::from_secs(health_check_interval_secs),
            inner: Arc::new(Mutex::new(Inner {
                state: AdapterState::Disconnected,
                self_id: None,
                ws_writer_tx: None,
                pending_calls: HashMap::new(),
                event_tx: None,
                reader_handle: None,
                writer_handle: None,
                reconnect_handle: None,
            })),
            echo_counter: AtomicU64::new(1),
        }
    }

    /// Create from an [`AccountConfig`](ferroq_core::config::AccountConfig)'s backend section.
    pub fn from_backend_config(
        name: impl Into<String>,
        cfg: &ferroq_core::config::BackendConfig,
    ) -> Self {
        Self::new(
            name,
            &cfg.url,
            &cfg.access_token,
            cfg.reconnect_interval,
            cfg.health_check_interval,
        )
    }

    /// Build a WebSocket request with optional auth header.
    fn build_request(&self) -> Result<tokio_tungstenite::tungstenite::http::Request<()>, GatewayError> {
        let mut request = self
            .url
            .as_str()
            .into_client_request()
            .map_err(|e| GatewayError::Connection(format!("invalid url: {e}")))?;

        if !self.access_token.is_empty() {
            request.headers_mut().insert(
                "Authorization",
                HeaderValue::from_str(&format!("Bearer {}", self.access_token))
                    .map_err(|e| GatewayError::Connection(format!("invalid token: {e}")))?,
            );
        }

        Ok(request)
    }

    /// Establish a WebSocket connection. Returns split (write, read) halves.
    async fn establish_ws(
        &self,
    ) -> Result<
        (
            SplitSink<WsStream, WsMessage>,
            SplitStream<WsStream>,
        ),
        GatewayError,
    > {
        let request = self.build_request()?;
        info!(url = %self.url, name = %self.name, "connecting to Lagrange backend");

        let (ws_stream, _response) = connect_async(request)
            .await
            .map_err(|e| GatewayError::Connection(format!("websocket connect failed: {e}")))?;

        info!(url = %self.url, name = %self.name, "connected to Lagrange backend");
        Ok(ws_stream.split())
    }

    /// Spawn the read and write tasks for an active WS connection.
    fn spawn_ws_tasks(
        &self,
        ws_write: SplitSink<WsStream, WsMessage>,
        ws_read: SplitStream<WsStream>,
    ) {
        let inner = Arc::clone(&self.inner);
        let name = self.name.clone();
        let url = self.url.clone();
        let reconnect_interval = self.reconnect_interval;

        // Writer task: takes messages from the channel and writes to WS.
        let (ws_writer_tx, ws_writer_rx) = mpsc::unbounded_channel::<WsMessage>();
        let writer_handle = tokio::spawn(Self::writer_loop(ws_write, ws_writer_rx));

        // Reader task: reads from WS, classifies messages as events or API responses.
        let inner_clone = Arc::clone(&inner);
        let name_clone = name.clone();
        let reader_handle = tokio::spawn(Self::reader_loop(
            ws_read,
            inner_clone,
            name_clone.clone(),
        ));

        // Store handles.
        let mut guard = inner.lock();
        guard.ws_writer_tx = Some(ws_writer_tx);
        guard.reader_handle = Some(reader_handle);
        guard.writer_handle = Some(writer_handle);
        guard.state = AdapterState::Connected;

        // Spawn a monitor task that detects reader/writer exit and triggers reconnect.
        let inner_monitor = Arc::clone(&self.inner);
        let adapter_url = url.clone();
        let adapter_name = name.clone();
        let adapter_access_token = self.access_token.clone();
        let adapter_inner = Arc::clone(&self.inner);

        // We need self reference for reconnect. Clone relevant fields.
        let reconnect_handle = tokio::spawn(async move {
            // Wait for the reader to end (= connection lost).
            let _reader_h = {
                let g = inner_monitor.lock();
                g.reader_handle.as_ref().map(|h| h.abort_handle())
            };

            // Wait by polling state — we check every second.
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let state = inner_monitor.lock().state;
                if state != AdapterState::Connected {
                    break;
                }
            }

            // The reader is done. Attempt reconnect loop.
            info!(name = %adapter_name, "connection lost, starting reconnect loop");
            {
                let mut g = adapter_inner.lock();
                g.state = AdapterState::Reconnecting;
                g.ws_writer_tx = None;
            }

            loop {
                info!(
                    name = %adapter_name,
                    url = %adapter_url,
                    delay_secs = reconnect_interval.as_secs(),
                    "attempting reconnect"
                );
                tokio::time::sleep(reconnect_interval).await;

                // Try to connect.
                let mut request = match adapter_url
                    .as_str()
                    .into_client_request()
                {
                    Ok(r) => r,
                    Err(e) => {
                        error!(name = %adapter_name, "invalid url on reconnect: {e}");
                        continue;
                    }
                };

                if !adapter_access_token.is_empty() {
                    if let Ok(v) = HeaderValue::from_str(&format!("Bearer {}", adapter_access_token)) {
                        request.headers_mut().insert("Authorization", v);
                    }
                }

                match connect_async(request).await {
                    Ok((ws_stream, _)) => {
                        info!(name = %adapter_name, "reconnected to Lagrange backend");
                        let (ws_w, ws_r) = ws_stream.split();

                        let (tx, rx) = mpsc::unbounded_channel::<WsMessage>();
                        let wh = tokio::spawn(Self::writer_loop(ws_w, rx));
                        let rh = tokio::spawn(Self::reader_loop(
                            ws_r,
                            Arc::clone(&adapter_inner),
                            adapter_name.clone(),
                        ));

                        let mut g = adapter_inner.lock();
                        g.ws_writer_tx = Some(tx);
                        g.writer_handle = Some(wh);
                        g.reader_handle = Some(rh);
                        g.state = AdapterState::Connected;
                        break;
                    }
                    Err(e) => {
                        warn!(name = %adapter_name, "reconnect failed: {e}");
                    }
                }
            }
        });

        guard.reconnect_handle = Some(reconnect_handle);
    }

    /// Writer loop: sends queued messages over the WS connection.
    async fn writer_loop(
        mut ws_write: SplitSink<WsStream, WsMessage>,
        mut rx: mpsc::UnboundedReceiver<WsMessage>,
    ) {
        while let Some(msg) = rx.recv().await {
            if let Err(e) = ws_write.send(msg).await {
                error!("ws write error: {e}");
                break;
            }
        }
        debug!("writer loop exited");
    }

    /// Reader loop: receives WS messages, classifies as event or API response.
    async fn reader_loop(
        mut ws_read: SplitStream<WsStream>,
        inner: Arc<Mutex<Inner>>,
        name: String,
    ) {
        while let Some(result) = ws_read.next().await {
            match result {
                Ok(WsMessage::Text(text)) => {
                    Self::handle_text_message(&text, &inner, &name);
                }
                Ok(WsMessage::Close(_)) => {
                    info!(name = %name, "backend sent close frame");
                    break;
                }
                Ok(WsMessage::Ping(data)) => {
                    // Send pong back.
                    let guard = inner.lock();
                    if let Some(tx) = &guard.ws_writer_tx {
                        let _ = tx.send(WsMessage::Pong(data));
                    }
                }
                Ok(_) => {} // Ignore binary, pong, etc.
                Err(e) => {
                    error!(name = %name, "ws read error: {e}");
                    break;
                }
            }
        }

        // Mark as disconnected so the monitor picks it up.
        let mut guard = inner.lock();
        if guard.state == AdapterState::Connected {
            guard.state = AdapterState::Reconnecting;
        }
        debug!(name = %name, "reader loop exited");
    }

    /// Handle a text message from the backend.
    fn handle_text_message(text: &str, inner: &Arc<Mutex<Inner>>, name: &str) {
        // Try to parse as JSON.
        let json: serde_json::Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(e) => {
                warn!(name = %name, "invalid JSON from backend: {e}");
                return;
            }
        };

        // If it has an "echo" field and a "status" field, it's an API response.
        if json.get("status").is_some() && json.get("echo").is_some() {
            Self::handle_api_response(json, inner, name);
        } else if json.get("post_type").is_some() {
            // It's an event — parse into our internal Event type.
            Self::handle_event(json, inner, name);
        } else {
            debug!(name = %name, "unknown message from backend: {}", &text[..text.len().min(200)]);
        }
    }

    /// Handle an API response from the backend.
    fn handle_api_response(json: serde_json::Value, inner: &Arc<Mutex<Inner>>, name: &str) {
        let echo = match json.get("echo").and_then(|v| v.as_str()) {
            Some(e) => e.to_string(),
            None => {
                // Try as number echo.
                match json.get("echo").and_then(|v| v.as_u64()) {
                    Some(n) => n.to_string(),
                    None => {
                        debug!(name = %name, "API response without string/number echo");
                        return;
                    }
                }
            }
        };

        // Parse the response.
        let response = ApiResponse {
            status: json
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            retcode: json
                .get("retcode")
                .and_then(|v| v.as_i64())
                .unwrap_or(-1) as i32,
            data: json.get("data").cloned().unwrap_or(serde_json::Value::Null),
            message: json
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            echo: json.get("echo").cloned(),
        };

        // Find and resolve the pending call.
        let sender = {
            let mut guard = inner.lock();
            guard.pending_calls.remove(&echo)
        };

        if let Some(tx) = sender {
            let _ = tx.send(response);
        } else {
            debug!(name = %name, echo = %echo, "API response for unknown echo (already timed out?)");
        }
    }

    /// Handle an event from the backend.
    fn handle_event(json: serde_json::Value, inner: &Arc<Mutex<Inner>>, name: &str) {
        match onebot_v11::parse_event(json) {
            Ok(event) => {
                // Update self_id if we see it for the first time.
                let event_tx = {
                    let mut guard = inner.lock();
                    if guard.self_id.is_none() {
                        guard.self_id = Some(event.self_id());
                        info!(name = %name, self_id = event.self_id(), "learned self_id from backend");
                    }
                    guard.event_tx.clone()
                };

                if let Some(tx) = event_tx {
                    if tx.send(event).is_err() {
                        warn!(name = %name, "event_tx closed, dropping event");
                    }
                }
            }
            Err(e) => {
                warn!(name = %name, "failed to parse event: {e}");
            }
        }
    }

    /// Generate a unique echo string for correlating API requests/responses.
    fn next_echo(&self) -> String {
        self.echo_counter
            .fetch_add(1, Ordering::Relaxed)
            .to_string()
    }
}

#[async_trait]
impl BackendAdapter for LagrangeAdapter {
    fn info(&self) -> AdapterInfo {
        let guard = self.inner.lock();
        AdapterInfo {
            name: self.name.clone(),
            backend_type: "lagrange".to_string(),
            url: self.url.clone(),
            state: guard.state,
            self_id: guard.self_id,
        }
    }

    async fn connect(&self, event_tx: mpsc::UnboundedSender<Event>) -> Result<(), GatewayError> {
        {
            let mut guard = self.inner.lock();
            guard.state = AdapterState::Connecting;
            guard.event_tx = Some(event_tx);
        }

        let (ws_write, ws_read) = self.establish_ws().await?;
        self.spawn_ws_tasks(ws_write, ws_read);
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), GatewayError> {
        let mut guard = self.inner.lock();

        // Send close frame.
        if let Some(tx) = guard.ws_writer_tx.take() {
            let _ = tx.send(WsMessage::Close(None));
        }

        // Abort tasks.
        if let Some(h) = guard.reader_handle.take() {
            h.abort();
        }
        if let Some(h) = guard.writer_handle.take() {
            h.abort();
        }
        if let Some(h) = guard.reconnect_handle.take() {
            h.abort();
        }

        guard.state = AdapterState::Disconnected;
        guard.pending_calls.clear();
        info!(name = %self.name, "disconnected from Lagrange backend");
        Ok(())
    }

    async fn call_api(&self, mut request: ApiRequest) -> Result<ApiResponse, GatewayError> {
        let echo = self.next_echo();

        // Set the echo field on the request.
        request.echo = Some(serde_json::Value::String(echo.clone()));

        let json = serde_json::to_string(&request)
            .map_err(GatewayError::Serialization)?;

        let (resp_tx, resp_rx) = oneshot::channel();

        // Register the pending call and send the message.
        {
            let mut guard = self.inner.lock();
            if guard.state != AdapterState::Connected {
                return Err(GatewayError::Connection(format!(
                    "adapter '{}' is not connected (state: {})",
                    self.name,
                    guard.state
                )));
            }

            guard.pending_calls.insert(echo.clone(), resp_tx);

            if let Some(tx) = &guard.ws_writer_tx {
                tx.send(WsMessage::Text(json.into()))
                    .map_err(|_| GatewayError::Connection("ws writer channel closed".to_string()))?;
            } else {
                guard.pending_calls.remove(&echo);
                return Err(GatewayError::Connection("no ws writer available".to_string()));
            }
        }

        // Wait for the response with a timeout.
        match tokio::time::timeout(Duration::from_secs(30), resp_rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(GatewayError::Connection(
                "API response channel dropped (connection lost?)".to_string(),
            )),
            Err(_) => {
                // Timeout — remove the pending call.
                let mut guard = self.inner.lock();
                guard.pending_calls.remove(&echo);
                Err(GatewayError::Connection(format!(
                    "API call '{}' timed out after 30s",
                    request.action
                )))
            }
        }
    }

    async fn health_check(&self) -> bool {
        let request = ApiRequest {
            action: "get_status".to_string(),
            params: serde_json::Value::Object(Default::default()),
            echo: None,
            self_id: None,
        };

        match self.call_api(request).await {
            Ok(resp) => resp.retcode == 0,
            Err(_) => false,
        }
    }
}
