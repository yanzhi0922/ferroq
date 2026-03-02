//! Gateway management API.
//!
//! Provides REST endpoints for account management, message queries,
//! and runtime control. These are served under `/api/...`.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Json;
use serde::Serialize;
use tracing::warn;

use crate::router::ApiRouter;
use crate::stats::RuntimeStats;
use crate::storage::{MessageQuery, MessageStore};

/// Shared state for the management API handlers.
pub struct ManagementState {
    pub router: Arc<ApiRouter>,
    pub stats: Arc<RuntimeStats>,
    pub store: Option<Arc<MessageStore>>,
}

/// Build the management API router.
///
/// Endpoints:
/// - `GET /api/accounts` — list all registered backend adapters
/// - `GET /api/messages` — query stored messages
/// - `GET /api/stats` — runtime statistics (alias for /health)
pub fn management_routes(
    router: Arc<ApiRouter>,
    stats: Arc<RuntimeStats>,
    store: Option<Arc<MessageStore>>,
) -> axum::Router {
    let state = Arc::new(ManagementState {
        router,
        stats,
        store,
    });

    axum::Router::new()
        .route("/accounts", get(handle_list_accounts))
        .route("/messages", get(handle_query_messages))
        .route("/stats", get(handle_stats))
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
