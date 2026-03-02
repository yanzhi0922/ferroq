//! Runtime statistics and health status types.
//!
//! These types are collected by the runtime and served via the
//! `/health` and `/api/status` HTTP endpoints.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use ferroq_core::adapter::AdapterState;
use parking_lot::RwLock;
use serde::Serialize;

/// Runtime statistics that get updated by the gateway components.
pub struct RuntimeStats {
    start_time: Instant,
    /// Total events forwarded through the bus.
    pub events_total: AtomicU64,
    /// Total API calls routed.
    pub api_calls_total: AtomicU64,
    /// Total active WS connections (forward WS clients).
    pub ws_connections: AtomicU64,
    /// Total messages persisted to storage.
    pub messages_stored: AtomicU64,
    /// Whether message storage is enabled.
    storage_enabled: bool,
    /// Adapter status snapshots.
    adapters: RwLock<Vec<AdapterSnapshot>>,
}

/// A snapshot of adapter status at a point in time.
#[derive(Debug, Clone, Serialize)]
pub struct AdapterSnapshot {
    pub name: String,
    pub backend_type: String,
    pub url: String,
    pub state: AdapterState,
    pub self_id: Option<i64>,
}

/// Full health check response body.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub uptime_secs: u64,
    pub events_total: u64,
    pub api_calls_total: u64,
    pub ws_connections: u64,
    pub messages_stored: u64,
    pub storage_enabled: bool,
    pub adapters: Vec<AdapterSnapshot>,
}

impl RuntimeStats {
    pub fn new() -> Self {
        Self::with_storage(false)
    }

    /// Create stats with storage-enabled flag.
    pub fn with_storage(storage_enabled: bool) -> Self {
        Self {
            start_time: Instant::now(),
            events_total: AtomicU64::new(0),
            api_calls_total: AtomicU64::new(0),
            ws_connections: AtomicU64::new(0),
            messages_stored: AtomicU64::new(0),
            storage_enabled,
            adapters: RwLock::new(Vec::new()),
        }
    }

    /// Record a forwarded event.
    pub fn record_event(&self) {
        self.events_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a routed API call.
    pub fn record_api_call(&self) {
        self.api_calls_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment active WS connections.
    pub fn ws_connect(&self) {
        self.ws_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement active WS connections.
    pub fn ws_disconnect(&self) {
        self.ws_connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// Record a stored message.
    pub fn record_message_stored(&self) {
        self.messages_stored.fetch_add(1, Ordering::Relaxed);
    }

    /// Update the adapter snapshots.
    pub fn update_adapters(&self, snapshots: Vec<AdapterSnapshot>) {
        *self.adapters.write() = snapshots;
    }

    /// Build a full health response.
    pub fn health(&self) -> HealthResponse {
        HealthResponse {
            status: "ok",
            version: env!("CARGO_PKG_VERSION"),
            uptime_secs: self.start_time.elapsed().as_secs(),
            events_total: self.events_total.load(Ordering::Relaxed),
            api_calls_total: self.api_calls_total.load(Ordering::Relaxed),
            ws_connections: self.ws_connections.load(Ordering::Relaxed),
            messages_stored: self.messages_stored.load(Ordering::Relaxed),
            storage_enabled: self.storage_enabled,
            adapters: self.adapters.read().clone(),
        }
    }
}

impl Default for RuntimeStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a shared `RuntimeStats` wrapped in `Arc`.
pub fn new_shared_stats() -> Arc<RuntimeStats> {
    Arc::new(RuntimeStats::new())
}
