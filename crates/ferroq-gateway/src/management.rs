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

use crate::router::ApiRouter;
use crate::stats::RuntimeStats;
use crate::storage::{MessageQuery, MessageStore};

/// Shared state for the management API handlers.
pub struct ManagementState {
    pub router: Arc<ApiRouter>,
    pub stats: Arc<RuntimeStats>,
    pub store: Option<Arc<MessageStore>>,
    pub config_path: Option<PathBuf>,
}

/// Build the management API router.
///
/// Endpoints:
/// - `GET  /api/accounts` — list all registered backend adapters
/// - `GET  /api/messages` — query stored messages
/// - `GET  /api/stats`    — runtime statistics
/// - `POST /api/reload`   — reload configuration file
/// - `GET  /api/config`   — view current (sanitized) configuration
pub fn management_routes(
    router: Arc<ApiRouter>,
    stats: Arc<RuntimeStats>,
    store: Option<Arc<MessageStore>>,
    config_path: Option<PathBuf>,
) -> axum::Router {
    let state = Arc::new(ManagementState {
        router,
        stats,
        store,
        config_path,
    });

    axum::Router::new()
        .route("/accounts", get(handle_list_accounts))
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

    // Config is valid. Log it. Full dynamic reload of adapters/servers is
    // complex and deferred to a later phase. For now we validate + report.
    info!(
        accounts = config.accounts.len(),
        "config reload: validated successfully (hot-swap pending)"
    );

    Json(serde_json::json!({
        "status": "ok",
        "message": "configuration validated successfully — full hot-reload coming in a future release",
        "warnings": warnings,
        "accounts": config.accounts.len(),
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
