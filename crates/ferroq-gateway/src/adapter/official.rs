//! Official Bot API backend adapter (HTTP).
//!
//! This adapter is API-first: it forwards action calls over HTTP and normalizes
//! responses into [`ApiResponse`]. Event streaming is backend-specific and not
//! guaranteed for official APIs, so this adapter currently does not emit events.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use ferroq_core::adapter::{AdapterInfo, AdapterState, BackendAdapter};
use ferroq_core::api::{ApiRequest, ApiResponse};
use ferroq_core::error::GatewayError;
use ferroq_core::event::Event;
use parking_lot::Mutex;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// API routing mode discovered for this backend URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApiPathMode {
    /// Use URL template replacement if base URL contains `{action}`.
    Template,
    /// POST `${base}/api/{action}`.
    ApiPrefixed,
    /// POST `${base}/{action}`.
    DirectAction,
    /// POST `${base}` with full body `{ action, params, echo, self_id }`.
    LegacyBody,
}

struct Inner {
    state: AdapterState,
    self_id: Option<i64>,
    event_tx: Option<mpsc::UnboundedSender<Event>>,
    preferred_mode: Option<ApiPathMode>,
}

/// Official Bot API adapter over HTTP.
pub struct OfficialAdapter {
    name: String,
    url: String,
    access_token: String,
    connect_timeout: Duration,
    api_timeout: Duration,
    client: reqwest::Client,
    inner: Arc<Mutex<Inner>>,
}

impl OfficialAdapter {
    /// Create a new official adapter.
    pub fn new(
        name: impl Into<String>,
        url: impl Into<String>,
        access_token: impl Into<String>,
        connect_timeout_secs: u64,
        api_timeout_secs: u64,
    ) -> Result<Self, GatewayError> {
        let connect_timeout = Duration::from_secs(connect_timeout_secs);
        let api_timeout = Duration::from_secs(api_timeout_secs);
        let client = reqwest::Client::builder()
            .connect_timeout(connect_timeout)
            .build()
            .map_err(|e| GatewayError::Http(format!("failed to build HTTP client: {e}")))?;

        Ok(Self {
            name: name.into(),
            url: url.into(),
            access_token: access_token.into(),
            connect_timeout,
            api_timeout,
            client,
            inner: Arc::new(Mutex::new(Inner {
                state: AdapterState::Disconnected,
                self_id: None,
                event_tx: None,
                preferred_mode: None,
            })),
        })
    }

    /// Create from an [`AccountConfig`](ferroq_core::config::AccountConfig)'s backend section.
    pub fn from_backend_config(
        name: impl Into<String>,
        cfg: &ferroq_core::config::BackendConfig,
    ) -> Result<Self, GatewayError> {
        Self::new(
            name,
            &cfg.url,
            &cfg.access_token,
            cfg.connect_timeout,
            cfg.api_timeout,
        )
    }

    fn normalize_base_url(&self) -> &str {
        self.url.trim_end_matches('/')
    }

    fn candidate_modes(&self) -> Vec<ApiPathMode> {
        let preferred = self.inner.lock().preferred_mode;
        let mut modes = Vec::with_capacity(4);

        if let Some(m) = preferred {
            modes.push(m);
        }
        if self.url.contains("{action}") && !modes.contains(&ApiPathMode::Template) {
            modes.push(ApiPathMode::Template);
        }
        if !modes.contains(&ApiPathMode::ApiPrefixed) {
            modes.push(ApiPathMode::ApiPrefixed);
        }
        if !modes.contains(&ApiPathMode::DirectAction) {
            modes.push(ApiPathMode::DirectAction);
        }
        if !modes.contains(&ApiPathMode::LegacyBody) {
            modes.push(ApiPathMode::LegacyBody);
        }
        modes
    }

    fn build_action_url(&self, action: &str, mode: ApiPathMode) -> String {
        let base = self.normalize_base_url();
        match mode {
            ApiPathMode::Template => self.url.replace("{action}", action),
            ApiPathMode::ApiPrefixed => {
                if base.ends_with("/api") {
                    format!("{base}/{action}")
                } else {
                    format!("{base}/api/{action}")
                }
            }
            ApiPathMode::DirectAction => format!("{base}/{action}"),
            ApiPathMode::LegacyBody => base.to_string(),
        }
    }

    fn apply_auth(
        &self,
        req: reqwest::RequestBuilder,
        timeout: Duration,
    ) -> reqwest::RequestBuilder {
        let req = req.timeout(timeout);
        if self.access_token.is_empty() {
            req
        } else {
            req.bearer_auth(&self.access_token)
        }
    }

    fn parse_api_response(
        &self,
        action: &str,
        status: reqwest::StatusCode,
        body: &str,
        request_echo: Option<serde_json::Value>,
    ) -> Result<ApiResponse, GatewayError> {
        if !status.is_success() {
            return Err(GatewayError::Http(format!(
                "official API call failed: action={action}, status={status}, body={}",
                truncate_body(body, 256)
            )));
        }

        if body.trim().is_empty() {
            return Ok(ApiResponse::ok(serde_json::Value::Null).with_echo(request_echo));
        }

        if let Ok(mut resp) = serde_json::from_str::<ApiResponse>(body) {
            if resp.echo.is_none() {
                resp.echo = request_echo;
            }
            return Ok(resp);
        }

        let value: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| GatewayError::Http(format!("invalid JSON response for {action}: {e}")))?;

        let status_text = value
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("ok")
            .to_string();
        let retcode = value.get("retcode").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let data = value.get("data").cloned().unwrap_or_else(|| value.clone());
        let message = value
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let echo = value.get("echo").cloned().or(request_echo);

        Ok(ApiResponse {
            status: status_text,
            retcode,
            data,
            message,
            echo,
        })
    }

    async fn call_with_mode(
        &self,
        mode: ApiPathMode,
        request: &ApiRequest,
    ) -> Result<ApiResponse, GatewayError> {
        let url = self.build_action_url(&request.action, mode);
        let req = self.client.post(url);
        let req = self.apply_auth(req, self.api_timeout);
        let req = match mode {
            ApiPathMode::LegacyBody => req.json(request),
            _ => req.json(&request.params),
        };

        let response = req.send().await.map_err(|e| {
            GatewayError::Http(format!(
                "official API request failed (mode={mode:?}, action={}): {e}",
                request.action
            ))
        })?;
        let status = response.status();
        let body = response.text().await.map_err(|e| {
            GatewayError::Http(format!(
                "failed reading official API response body (action={}): {e}",
                request.action
            ))
        })?;

        self.parse_api_response(&request.action, status, &body, request.echo.clone())
    }

    async fn discover_and_call(&self, request: &ApiRequest) -> Result<ApiResponse, GatewayError> {
        let mut last_error: Option<GatewayError> = None;
        for mode in self.candidate_modes() {
            match self.call_with_mode(mode, request).await {
                Ok(resp) => {
                    self.inner.lock().preferred_mode = Some(mode);
                    debug!(name = %self.name, mode = ?mode, "official adapter API mode selected");
                    return Ok(resp);
                }
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| {
            GatewayError::Http(format!(
                "official backend '{}' request failed for action '{}'",
                self.name, request.action
            ))
        }))
    }

    fn maybe_update_self_id_from_response(&self, action: &str, response: &ApiResponse) {
        if action != "get_login_info" {
            return;
        }
        if let Some(self_id) = response.data.get("user_id").and_then(|v| v.as_i64()) {
            let mut inner = self.inner.lock();
            if inner.self_id != Some(self_id) {
                inner.self_id = Some(self_id);
                info!(name = %self.name, self_id, "official adapter learned self_id");
            }
        }
    }

    async fn probe_backend(&self) -> Option<i64> {
        let probe = ApiRequest {
            action: "get_login_info".to_string(),
            params: serde_json::Value::Object(Default::default()),
            echo: None,
            self_id: None,
        };
        match self.discover_and_call(&probe).await {
            Ok(resp) if resp.retcode == 0 => resp.data.get("user_id").and_then(|v| v.as_i64()),
            Ok(resp) => {
                debug!(
                    name = %self.name,
                    retcode = resp.retcode,
                    "official adapter probe returned non-zero retcode"
                );
                None
            }
            Err(e) => {
                debug!(name = %self.name, error = %e, "official adapter probe failed");
                None
            }
        }
    }
}

#[async_trait]
impl BackendAdapter for OfficialAdapter {
    fn info(&self) -> AdapterInfo {
        let inner = self.inner.lock();
        AdapterInfo {
            name: self.name.clone(),
            backend_type: "official".to_string(),
            url: self.url.clone(),
            state: inner.state,
            self_id: inner.self_id,
        }
    }

    async fn connect(&self, event_tx: mpsc::UnboundedSender<Event>) -> Result<(), GatewayError> {
        {
            let mut inner = self.inner.lock();
            inner.state = AdapterState::Connecting;
            inner.event_tx = Some(event_tx);
        }

        // HTTP backend has no persistent socket; we do a lightweight probe and
        // then move to Connected so runtime API routing is immediately available.
        if let Some(self_id) = self.probe_backend().await {
            self.inner.lock().self_id = Some(self_id);
        } else {
            warn!(
                name = %self.name,
                timeout_secs = self.connect_timeout.as_secs(),
                "official adapter probe did not return self_id, continuing in API-only mode"
            );
        }

        self.inner.lock().state = AdapterState::Connected;
        info!(name = %self.name, url = %self.url, "official adapter connected");
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), GatewayError> {
        let mut inner = self.inner.lock();
        inner.state = AdapterState::Disconnected;
        inner.event_tx = None;
        info!(name = %self.name, "official adapter disconnected");
        Ok(())
    }

    async fn call_api(&self, request: ApiRequest) -> Result<ApiResponse, GatewayError> {
        let state = self.inner.lock().state;
        if state != AdapterState::Connected {
            return Err(GatewayError::Connection(format!(
                "adapter '{}' is not connected (state: {})",
                self.name, state
            )));
        }

        let action = request.action.clone();
        let response = self.discover_and_call(&request).await?;
        self.maybe_update_self_id_from_response(&action, &response);
        Ok(response)
    }

    async fn health_check(&self) -> bool {
        if self.inner.lock().state != AdapterState::Connected {
            return false;
        }

        let req = ApiRequest {
            action: "get_status".to_string(),
            params: serde_json::Value::Object(Default::default()),
            echo: None,
            self_id: None,
        };
        match self.discover_and_call(&req).await {
            Ok(resp) => resp.retcode == 0,
            Err(_) => false,
        }
    }
}

fn truncate_body(body: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (count, ch) in body.chars().enumerate() {
        if count >= max_chars {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::extract::State;
    use axum::response::IntoResponse;
    use axum::routing::post;
    use serde_json::json;
    use tokio::net::TcpListener;

    async fn start_mock_official_server() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let app = axum::Router::new()
            .route("/api/get_login_info", post(mock_get_login_info))
            .route("/api/get_status", post(mock_get_status))
            .route("/", post(mock_legacy));

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock official server");
        let addr = listener.local_addr().expect("local addr");
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        tokio::time::sleep(Duration::from_millis(30)).await;
        (addr, handle)
    }

    async fn mock_get_login_info(body: String) -> impl IntoResponse {
        let params: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        if !params.is_object() {
            return axum::Json(ApiResponse::fail(1400, "invalid params"));
        }
        axum::Json(ApiResponse::ok(json!({
            "user_id": 20260001_i64,
            "nickname": "OfficialBot",
        })))
    }

    async fn mock_get_status() -> impl IntoResponse {
        axum::Json(ApiResponse::ok(json!({"online": true, "good": true})))
    }

    async fn mock_legacy(State(_): State<()>, body: String) -> impl IntoResponse {
        let raw: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        let action = raw
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        match action {
            "get_status" => axum::Json(ApiResponse::ok(json!({"online": true}))),
            _ => axum::Json(ApiResponse::fail(1404, "unknown action")),
        }
    }

    #[tokio::test]
    async fn official_adapter_connect_and_call_api() {
        let (addr, handle) = start_mock_official_server().await;
        let adapter = OfficialAdapter::new("official-test", format!("http://{addr}"), "", 2, 2)
            .expect("create adapter");
        let (event_tx, _event_rx) = mpsc::unbounded_channel();

        adapter.connect(event_tx).await.expect("connect");
        let resp = adapter
            .call_api(ApiRequest {
                action: "get_login_info".to_string(),
                params: json!({}),
                echo: None,
                self_id: None,
            })
            .await
            .expect("call_api");
        assert_eq!(resp.retcode, 0);
        assert_eq!(resp.data["user_id"], 20260001_i64);
        assert_eq!(adapter.info().self_id, Some(20260001_i64));
        assert!(adapter.health_check().await);

        adapter.disconnect().await.expect("disconnect");
        handle.abort();
    }

    #[tokio::test]
    async fn official_adapter_fallbacks_to_legacy_body_mode() {
        let app = axum::Router::new().route("/", post(mock_legacy));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        tokio::time::sleep(Duration::from_millis(30)).await;

        let adapter = OfficialAdapter::new("official-legacy", format!("http://{addr}"), "", 2, 2)
            .expect("create adapter");
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        adapter.connect(event_tx).await.expect("connect");

        let resp = adapter
            .call_api(ApiRequest {
                action: "get_status".to_string(),
                params: json!({}),
                echo: None,
                self_id: None,
            })
            .await
            .expect("legacy mode call_api");
        assert_eq!(resp.retcode, 0);
        assert_eq!(resp.data["online"], true);

        handle.abort();
    }
}
