//! Shared event-forwarding logic.
//!
//! Both `GatewayRuntime` and `AdapterManager` need to forward events from
//! adapter channels to the event bus with optional deduplication. This module
//! provides a single implementation to avoid duplication.

use std::sync::Arc;

use ferroq_core::event::Event;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::bus::EventBus;
use crate::dedup::DedupFilter;
use crate::stats::RuntimeStats;

/// Forward events from an adapter's unbounded channel to the event bus.
///
/// Performs optional deduplication and records per-adapter stats.  The task
/// runs until the sender half of `event_rx` is dropped.
pub async fn forward_events(
    mut event_rx: mpsc::UnboundedReceiver<Event>,
    bus: Arc<EventBus>,
    stats: Arc<RuntimeStats>,
    dedup: Option<Arc<DedupFilter>>,
    adapter_name: String,
) {
    let mut count: u64 = 0;
    while let Some(event) = event_rx.recv().await {
        // Deduplication check — drop if this event was already seen.
        if let Some(ref filter) = dedup {
            if filter.is_duplicate(&event) {
                stats.record_event_deduplicated();
                continue;
            }
        }
        count += 1;
        stats.record_event_for(&adapter_name);
        if count % 1000 == 0 {
            info!(
                adapter = %adapter_name,
                total_events = count,
                "event forwarding progress"
            );
        }
        bus.publish(event);
    }
    warn!(adapter = %adapter_name, total = count, "event forwarding channel closed");
}
