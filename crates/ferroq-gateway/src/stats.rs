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
    /// Active WS connections (forward WS clients).
    pub ws_connections: AtomicU64,
    /// Total WS connections served (lifetime).
    pub ws_connections_total: AtomicU64,
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
    /// Whether the last health check succeeded.
    pub healthy: bool,
    /// Latency of the last health check in milliseconds.
    pub health_check_ms: Option<u64>,
    /// Unix timestamp of the last health check.
    pub last_health_check: Option<u64>,
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
    pub ws_connections_total: u64,
    pub messages_stored: u64,
    pub storage_enabled: bool,
    pub healthy_adapters: usize,
    pub total_adapters: usize,
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
            ws_connections_total: AtomicU64::new(0),
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
        self.ws_connections_total.fetch_add(1, Ordering::Relaxed);
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
        let adapters = self.adapters.read().clone();
        let healthy_adapters = adapters.iter().filter(|a| a.healthy).count();
        let total_adapters = adapters.len();
        HealthResponse {
            status: "ok",
            version: env!("CARGO_PKG_VERSION"),
            uptime_secs: self.start_time.elapsed().as_secs(),
            events_total: self.events_total.load(Ordering::Relaxed),
            api_calls_total: self.api_calls_total.load(Ordering::Relaxed),
            ws_connections: self.ws_connections.load(Ordering::Relaxed),
            ws_connections_total: self.ws_connections_total.load(Ordering::Relaxed),
            messages_stored: self.messages_stored.load(Ordering::Relaxed),
            storage_enabled: self.storage_enabled,
            healthy_adapters,
            total_adapters,
            adapters,
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

impl RuntimeStats {
    /// Render all metrics in Prometheus text exposition format.
    pub fn prometheus_metrics(&self) -> String {
        let health = self.health();
        let mut out = String::with_capacity(2048);

        // -- Gauge / Counter metrics --
        out.push_str("# HELP ferroq_uptime_seconds Gateway uptime in seconds.\n");
        out.push_str("# TYPE ferroq_uptime_seconds gauge\n");
        out.push_str(&format!("ferroq_uptime_seconds {}\n\n", health.uptime_secs));

        out.push_str("# HELP ferroq_events_total Total events forwarded through the bus.\n");
        out.push_str("# TYPE ferroq_events_total counter\n");
        out.push_str(&format!("ferroq_events_total {}\n\n", health.events_total));

        out.push_str("# HELP ferroq_api_calls_total Total API calls routed.\n");
        out.push_str("# TYPE ferroq_api_calls_total counter\n");
        out.push_str(&format!("ferroq_api_calls_total {}\n\n", health.api_calls_total));

        out.push_str("# HELP ferroq_ws_connections_active Active WebSocket connections.\n");
        out.push_str("# TYPE ferroq_ws_connections_active gauge\n");
        out.push_str(&format!("ferroq_ws_connections_active {}\n\n", health.ws_connections));

        out.push_str("# HELP ferroq_ws_connections_total Total WebSocket connections served (lifetime).\n");
        out.push_str("# TYPE ferroq_ws_connections_total counter\n");
        out.push_str(&format!("ferroq_ws_connections_total {}\n\n", health.ws_connections_total));

        out.push_str("# HELP ferroq_messages_stored_total Total messages persisted to storage.\n");
        out.push_str("# TYPE ferroq_messages_stored_total counter\n");
        out.push_str(&format!("ferroq_messages_stored_total {}\n\n", health.messages_stored));

        out.push_str("# HELP ferroq_storage_enabled Whether message storage is enabled (1=yes, 0=no).\n");
        out.push_str("# TYPE ferroq_storage_enabled gauge\n");
        out.push_str(&format!("ferroq_storage_enabled {}\n\n", if health.storage_enabled { 1 } else { 0 }));

        out.push_str("# HELP ferroq_adapters_total Total number of backend adapters.\n");
        out.push_str("# TYPE ferroq_adapters_total gauge\n");
        out.push_str(&format!("ferroq_adapters_total {}\n\n", health.total_adapters));

        out.push_str("# HELP ferroq_adapters_healthy Number of healthy backend adapters.\n");
        out.push_str("# TYPE ferroq_adapters_healthy gauge\n");
        out.push_str(&format!("ferroq_adapters_healthy {}\n\n", health.healthy_adapters));

        // -- Per-adapter metrics --
        if !health.adapters.is_empty() {
            out.push_str("# HELP ferroq_adapter_healthy Whether a backend adapter is healthy (1=yes, 0=no).\n");
            out.push_str("# TYPE ferroq_adapter_healthy gauge\n");
            for a in &health.adapters {
                out.push_str(&format!(
                    "ferroq_adapter_healthy{{name=\"{}\",type=\"{}\",state=\"{}\"}} {}\n",
                    a.name, a.backend_type, a.state, if a.healthy { 1 } else { 0 }
                ));
            }
            out.push('\n');

            out.push_str("# HELP ferroq_adapter_health_check_ms Latency of the last health check in milliseconds.\n");
            out.push_str("# TYPE ferroq_adapter_health_check_ms gauge\n");
            for a in &health.adapters {
                if let Some(ms) = a.health_check_ms {
                    out.push_str(&format!(
                        "ferroq_adapter_health_check_ms{{name=\"{}\"}} {}\n",
                        a.name, ms
                    ));
                }
            }
            out.push('\n');
        }

        out
    }
}
