//! Gateway management API.
//!
//! Provides REST endpoints for account management, message queries,
//! runtime control, and configuration reload. Served under `/api/...`.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Json;
use serde::Serialize;
use tracing::{info, warn};

use crate::adapter_manager::AdapterManager;
use crate::middleware::RateLimiter;
use crate::router::ApiRouter;
use crate::shared_config::SharedConfig;
use crate::stats::RuntimeStats;
use crate::storage::{MessageQuery, MessageStore};

/// Shared state for the management API handlers.
pub struct ManagementState {
    pub router: Arc<ApiRouter>,
    pub stats: Arc<RuntimeStats>,
    pub store: Option<Arc<MessageStore>>,
    pub config_path: Option<PathBuf>,
    pub shared_config: Arc<SharedConfig>,
    pub rate_limiter: Option<RateLimiter>,
    pub adapter_manager: Option<Arc<AdapterManager>>,
}

/// Build the management API router.
///
/// Endpoints:
/// - `GET  /api/accounts` — list all registered backend adapters
/// - `POST /api/accounts/add` — add a new adapter at runtime
/// - `POST /api/accounts/{name}/remove` — remove an adapter
/// - `POST /api/accounts/{name}/reconnect` — reconnect an adapter
/// - `GET  /api/messages` — query stored messages
/// - `GET  /api/stats`    — runtime statistics
/// - `POST /api/reload`   — reload configuration file
/// - `GET  /api/config`   — view current (sanitized) configuration
pub fn management_routes(
    router: Arc<ApiRouter>,
    stats: Arc<RuntimeStats>,
    store: Option<Arc<MessageStore>>,
    config_path: Option<PathBuf>,
    shared_config: Arc<SharedConfig>,
    rate_limiter: Option<RateLimiter>,
) -> axum::Router {
    management_routes_with_manager(router, stats, store, config_path, shared_config, rate_limiter, None)
}

/// Build the management API router with an optional adapter manager.
pub fn management_routes_with_manager(
    router: Arc<ApiRouter>,
    stats: Arc<RuntimeStats>,
    store: Option<Arc<MessageStore>>,
    config_path: Option<PathBuf>,
    shared_config: Arc<SharedConfig>,
    rate_limiter: Option<RateLimiter>,
    adapter_manager: Option<Arc<AdapterManager>>,
) -> axum::Router {
    let state = Arc::new(ManagementState {
        router,
        stats,
        store,
        config_path,
        shared_config,
        rate_limiter,
        adapter_manager,
    });

    axum::Router::new()
        .route("/accounts", get(handle_list_accounts))
        .route("/accounts/add", post(handle_add_adapter))
        .route("/accounts/{name}/remove", post(handle_remove_adapter))
        .route("/accounts/{name}/reconnect", post(handle_reconnect_adapter))
        .route("/messages", get(handle_query_messages))
        .route("/stats", get(handle_stats))
        .route("/reload", post(handle_reload))
        .route("/config", get(handle_config))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// GET /api/accounts
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct AccountEntry {
    name: String,
    backend_type: String,
    url: String,
    state: String,
    self_id: Option<i64>,
}

async fn handle_list_accounts(
    State(state): State<Arc<ManagementState>>,
) -> impl IntoResponse {
    let adapters = state.stats.health().adapters;
    let accounts: Vec<AccountEntry> = adapters
        .into_iter()
        .map(|a| AccountEntry {
            name: a.name,
            backend_type: a.backend_type,
            url: a.url,
            state: a.state.to_string(),
            self_id: a.self_id,
        })
        .collect();

    Json(serde_json::json!({
        "status": "ok",
        "data": accounts,
    }))
}

// ---------------------------------------------------------------------------
// GET /api/messages?self_id=&group_id=&user_id=&keyword=&limit=&offset=
// ---------------------------------------------------------------------------

async fn handle_query_messages(
    State(state): State<Arc<ManagementState>>,
    Query(query): Query<MessageQuery>,
) -> impl IntoResponse {
    let Some(ref store) = state.store else {
        return Json(serde_json::json!({
            "status": "failed",
            "retcode": 1,
            "message": "message storage is not enabled",
        }));
    };

    match store.query(&query).await {
        Ok(result) => Json(serde_json::json!({
            "status": "ok",
            "data": {
                "total": result.total,
                "messages": result.messages,
            }
        })),
        Err(e) => {
            warn!("message query failed: {e}");
            Json(serde_json::json!({
                "status": "failed",
                "retcode": 2,
                "message": e.to_string(),
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// GET /api/stats
// ---------------------------------------------------------------------------

async fn handle_stats(
    State(state): State<Arc<ManagementState>>,
) -> impl IntoResponse {
    Json(state.stats.health())
}

// ---------------------------------------------------------------------------
// POST /api/reload
// ---------------------------------------------------------------------------

async fn handle_reload(
    State(state): State<Arc<ManagementState>>,
) -> impl IntoResponse {
    let Some(ref config_path) = state.config_path else {
        return Json(serde_json::json!({
            "status": "failed",
            "message": "config path not available",
        }));
    };

    // Re-read the config file.
    let config_str = match std::fs::read_to_string(config_path) {
        Ok(s) => s,
        Err(e) => {
            warn!("config reload: cannot read file: {e}");
            return Json(serde_json::json!({
                "status": "failed",
                "message": format!("cannot read config file: {e}"),
            }));
        }
    };

    let config: ferroq_core::config::AppConfig = match serde_yaml::from_str(&config_str) {
        Ok(c) => c,
        Err(e) => {
            warn!("config reload: parse error: {e}");
            return Json(serde_json::json!({
                "status": "failed",
                "message": format!("config parse error: {e}"),
            }));
        }
    };

    // Validate the new config.
    let issues = ferroq_core::validation::validate(&config);
    let errors: Vec<String> = issues
        .iter()
        .filter(|i| i.severity == ferroq_core::validation::Severity::Error)
        .map(|i| i.to_string())
        .collect();
    let warnings: Vec<String> = issues
        .iter()
        .filter(|i| i.severity == ferroq_core::validation::Severity::Warning)
        .map(|i| i.to_string())
        .collect();

    if !errors.is_empty() {
        return Json(serde_json::json!({
            "status": "failed",
            "message": "configuration has validation errors",
            "errors": errors,
            "warnings": warnings,
        }));
    }

    // ----- Apply hot-reloadable settings -----
    let mut changes: Vec<String> = Vec::new();

    // Access token.
    let current_token = state.shared_config.access_token();
    if config.server.access_token != current_token {
        state.shared_config.set_access_token(config.server.access_token.clone());
        changes.push("access_token updated".into());
        info!("config reload: access_token updated");
    }

    // Rate limit parameters.
    if let Some(ref rl) = state.rate_limiter {
        rl.update_config(
            config.server.rate_limit.requests_per_second,
            config.server.rate_limit.burst,
        );
        changes.push(format!(
            "rate_limit updated: rps={}, burst={}",
            config.server.rate_limit.requests_per_second,
            config.server.rate_limit.burst,
        ));
        info!(
            rps = config.server.rate_limit.requests_per_second,
            burst = config.server.rate_limit.burst,
            "config reload: rate limit updated"
        );
    }

    info!(
        changes = changes.len(),
        accounts = config.accounts.len(),
        "config reload: applied successfully"
    );

    Json(serde_json::json!({
        "status": "ok",
        "message": "configuration reloaded",
        "changes": changes,
        "warnings": warnings,
        "note": "adapter and protocol changes require restart",
    }))
}

// ---------------------------------------------------------------------------
// GET /api/config
// ---------------------------------------------------------------------------

/// Returns the current configuration with secrets redacted.
async fn handle_config(
    State(state): State<Arc<ManagementState>>,
) -> impl IntoResponse {
    let Some(ref config_path) = state.config_path else {
        return Json(serde_json::json!({
            "status": "failed",
            "message": "config path not available",
        }));
    };

    let config_str = match std::fs::read_to_string(config_path) {
        Ok(s) => s,
        Err(e) => {
            return Json(serde_json::json!({
                "status": "failed",
                "message": format!("cannot read config file: {e}"),
            }));
        }
    };

    // Parse and re-serialize to sanitize secrets.
    let mut config: ferroq_core::config::AppConfig = match serde_yaml::from_str(&config_str) {
        Ok(c) => c,
        Err(e) => {
            return Json(serde_json::json!({
                "status": "failed",
                "message": format!("config parse error: {e}"),
            }));
        }
    };

    // Redact access tokens.
    if !config.server.access_token.is_empty() {
        config.server.access_token = "***".into();
    }
    for account in &mut config.accounts {
        if !account.backend.access_token.is_empty() {
            account.backend.access_token = "***".into();
        }
        if let Some(ref mut fb) = account.fallback {
            if !fb.access_token.is_empty() {
                fb.access_token = "***".into();
            }
        }
    }

    Json(serde_json::json!({
        "status": "ok",
        "data": config,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/accounts/add
// ---------------------------------------------------------------------------

/// Request body for adding a new adapter at runtime.
#[derive(serde::Deserialize)]
struct AddAdapterRequest {
    name: String,
    backend: ferroq_core::config::BackendConfig,
}

async fn handle_add_adapter(
    State(state): State<Arc<ManagementState>>,
    Json(body): Json<AddAdapterRequest>,
) -> impl IntoResponse {
    let Some(ref mgr) = state.adapter_manager else {
        return Json(serde_json::json!({
            "status": "failed",
            "retcode": 1,
            "message": "adapter manager is not available",
        }));
    };

    match mgr.add_adapter(&body.name, &body.backend).await {
        Ok(()) => Json(serde_json::json!({
            "status": "ok",
            "message": format!("adapter '{}' added", body.name),
        })),
        Err(e) => {
            warn!(name = %body.name, error = %e, "add adapter failed");
            Json(serde_json::json!({
                "status": "failed",
                "retcode": 2,
                "message": e.to_string(),
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// POST /api/accounts/{name}/remove
// ---------------------------------------------------------------------------

async fn handle_remove_adapter(
    State(state): State<Arc<ManagementState>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    let Some(ref mgr) = state.adapter_manager else {
        return Json(serde_json::json!({
            "status": "failed",
            "retcode": 1,
            "message": "adapter manager is not available",
        }));
    };

    match mgr.remove_adapter(&name).await {
        Ok(()) => Json(serde_json::json!({
            "status": "ok",
            "message": format!("adapter '{}' removed", name),
        })),
        Err(e) => {
            warn!(name = %name, error = %e, "remove adapter failed");
            Json(serde_json::json!({
                "status": "failed",
                "retcode": 2,
                "message": e.to_string(),
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// POST /api/accounts/{name}/reconnect
// ---------------------------------------------------------------------------

async fn handle_reconnect_adapter(
    State(state): State<Arc<ManagementState>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    let Some(ref mgr) = state.adapter_manager else {
        return Json(serde_json::json!({
            "status": "failed",
            "retcode": 1,
            "message": "adapter manager is not available",
        }));
    };

    match mgr.reconnect_adapter(&name).await {
        Ok(()) => Json(serde_json::json!({
            "status": "ok",
            "message": format!("adapter '{}' reconnected", name),
        })),
        Err(e) => {
            warn!(name = %name, error = %e, "reconnect adapter failed");
            Json(serde_json::json!({
                "status": "failed",
                "retcode": 2,
                "message": e.to_string(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request as HttpRequest, StatusCode};
    use tower::ServiceExt;

    use crate::router::ApiRouter;
    use crate::shared_config::SharedConfig;
    use crate::stats::{AdapterSnapshot, RuntimeStats};

    /// Build a management API router with sensible test defaults.
    fn build_test_app() -> (axum::Router, Arc<RuntimeStats>) {
        let router = Arc::new(ApiRouter::new());
        let stats = Arc::new(RuntimeStats::new());
        let shared_config = Arc::new(SharedConfig::new(String::new()));
        let app = management_routes_with_manager(
            router,
            stats.clone(),
            None,
            None,
            shared_config,
            None,
            None,
        );
        (app, stats)
    }

    #[tokio::test]
    async fn accounts_returns_empty_list() {
        let (app, _stats) = build_test_app();
        let req = HttpRequest::builder()
            .uri("/accounts")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
        assert!(json["data"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn accounts_lists_registered_adapters() {
        let (app, stats) = build_test_app();

        // Simulate an adapter snapshot in stats.
        stats.update_adapters(vec![AdapterSnapshot {
            name: "test-bot".into(),
            backend_type: "lagrange".into(),
            url: "ws://mock:1234".into(),
            state: ferroq_core::adapter::AdapterState::Connected,
            self_id: Some(42),
            healthy: true,
            health_check_ms: Some(5),
            last_health_check: None,
            events_total: 0,
            api_calls_total: 0,
        }]);

        let req = HttpRequest::builder()
            .uri("/accounts")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let accounts = json["data"].as_array().unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0]["name"], "test-bot");
        assert_eq!(accounts[0]["backend_type"], "lagrange");
        assert_eq!(accounts[0]["self_id"], 42);
    }

    #[tokio::test]
    async fn stats_returns_health_response() {
        let (app, stats) = build_test_app();
        // Generate some activity.
        stats.record_event_for("a");
        stats.record_api_call_for("a");
        stats.record_api_call_for("a");

        let req = HttpRequest::builder()
            .uri("/stats")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["events_total"], 1);
        assert_eq!(json["api_calls_total"], 2);
    }

    #[tokio::test]
    async fn messages_returns_error_when_storage_disabled() {
        let (app, _stats) = build_test_app();
        let req = HttpRequest::builder()
            .uri("/messages")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "failed");
        assert!(json["message"].as_str().unwrap().contains("not enabled"));
    }

    #[tokio::test]
    async fn reload_fails_without_config_path() {
        let (app, _stats) = build_test_app();
        let req = HttpRequest::builder()
            .method("POST")
            .uri("/reload")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "failed");
        assert!(json["message"].as_str().unwrap().contains("config path"));
    }

    #[tokio::test]
    async fn config_fails_without_config_path() {
        let (app, _stats) = build_test_app();
        let req = HttpRequest::builder()
            .uri("/config")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "failed");
        assert!(json["message"].as_str().unwrap().contains("config path"));
    }

    #[tokio::test]
    async fn reload_with_valid_config_file() {
        // Write a minimal valid config to a temp file.
        let dir = std::env::temp_dir().join("ferroq_test_reload");
        let _ = std::fs::create_dir_all(&dir);
        let config_file = dir.join("test_config.yaml");
        std::fs::write(
            &config_file,
            r#"
server:
  host: "0.0.0.0"
  port: 8080
  access_token: "new-token"
accounts: []
protocols: {}
storage:
  enabled: false
  path: "./data/messages.db"
  max_days: 30
logging:
  level: info
  console: true
"#,
        )
        .unwrap();

        let router = Arc::new(ApiRouter::new());
        let stats = Arc::new(RuntimeStats::new());
        let shared_config = Arc::new(SharedConfig::new("old-token".into()));
        let app = management_routes(
            router,
            stats,
            None,
            Some(config_file.clone()),
            shared_config.clone(),
            None,
        );

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/reload")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");

        // Access token should have been updated.
        assert_eq!(shared_config.access_token(), "new-token");

        // Cleanup.
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn config_endpoint_redacts_secrets() {
        let dir = std::env::temp_dir().join("ferroq_test_config");
        let _ = std::fs::create_dir_all(&dir);
        let config_file = dir.join("test_config.yaml");
        std::fs::write(
            &config_file,
            r#"
server:
  host: "0.0.0.0"
  port: 8080
  access_token: "super-secret"
accounts:
  - name: "bot1"
    backend:
      type: lagrange
      url: "ws://localhost:1234"
      access_token: "backend-secret"
protocols: {}
storage:
  enabled: false
  path: "./data/messages.db"
  max_days: 30
logging:
  level: info
  console: true
"#,
        )
        .unwrap();

        let router = Arc::new(ApiRouter::new());
        let stats = Arc::new(RuntimeStats::new());
        let shared_config = Arc::new(SharedConfig::new(String::new()));
        let app = management_routes(
            router,
            stats,
            None,
            Some(config_file.clone()),
            shared_config,
            None,
        );

        let req = HttpRequest::builder()
            .uri("/config")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");

        // Secrets should be redacted.
        let data = &json["data"];
        assert_eq!(data["server"]["access_token"], "***");
        assert_eq!(data["accounts"][0]["backend"]["access_token"], "***");

        // But non-secrets should remain.
        assert_eq!(data["accounts"][0]["name"], "bot1");

        // Cleanup.
        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // Dynamic adapter management endpoint tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn add_adapter_fails_without_manager() {
        let (app, _stats) = build_test_app();
        let req = HttpRequest::builder()
            .method("POST")
            .uri("/accounts/add")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"name":"bot","backend":{"type":"lagrange","url":"ws://localhost:1234"}}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "failed");
        assert!(json["message"].as_str().unwrap().contains("not available"));
    }

    #[tokio::test]
    async fn remove_adapter_fails_without_manager() {
        let (app, _stats) = build_test_app();
        let req = HttpRequest::builder()
            .method("POST")
            .uri("/accounts/some-bot/remove")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "failed");
        assert!(json["message"].as_str().unwrap().contains("not available"));
    }

    #[tokio::test]
    async fn reconnect_adapter_fails_without_manager() {
        let (app, _stats) = build_test_app();
        let req = HttpRequest::builder()
            .method("POST")
            .uri("/accounts/some-bot/reconnect")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "failed");
        assert!(json["message"].as_str().unwrap().contains("not available"));
    }

    /// Build a test app with an AdapterManager wired in.
    fn build_test_app_with_manager() -> (axum::Router, Arc<AdapterManager>) {
        let router = Arc::new(ApiRouter::new());
        let stats = Arc::new(RuntimeStats::new());
        let shared_config = Arc::new(SharedConfig::new(String::new()));
        let bus = Arc::new(crate::bus::EventBus::new());
        let mgr = Arc::new(AdapterManager::new(
            bus,
            Arc::clone(&router),
            Arc::clone(&stats),
            None,
        ));
        let app = management_routes_with_manager(
            router,
            stats,
            None,
            None,
            shared_config,
            None,
            Some(Arc::clone(&mgr)),
        );
        (app, mgr)
    }

    #[tokio::test]
    async fn remove_adapter_not_found() {
        let (app, _mgr) = build_test_app_with_manager();
        let req = HttpRequest::builder()
            .method("POST")
            .uri("/accounts/nonexistent/remove")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "failed");
        assert!(json["message"].as_str().unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn reconnect_adapter_not_found() {
        let (app, _mgr) = build_test_app_with_manager();
        let req = HttpRequest::builder()
            .method("POST")
            .uri("/accounts/nonexistent/reconnect")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "failed");
        assert!(json["message"].as_str().unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn add_adapter_unknown_backend_type() {
        let (app, _mgr) = build_test_app_with_manager();
        let req = HttpRequest::builder()
            .method("POST")
            .uri("/accounts/add")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"bot","backend":{"type":"unknown","url":"ws://localhost:1234"}}"#,
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "failed");
        assert!(json["message"].as_str().unwrap().contains("unknown backend type"));
    }
}
