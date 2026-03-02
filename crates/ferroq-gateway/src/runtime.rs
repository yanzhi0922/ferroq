//! Gateway runtime — orchestrates adapters, event bus, and protocol servers.

use std::sync::Arc;

use ferroq_core::adapter::BackendAdapter;
use ferroq_core::config::AppConfig;
use ferroq_core::error::GatewayError;
use ferroq_core::event::Event;
use ferroq_core::protocol::ProtocolServer;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::adapter_manager::AdapterManager;
use crate::bus::EventBus;
use crate::dedup::DedupFilter;
use crate::router::ApiRouter;
use crate::stats::{AdapterSnapshot, RuntimeStats};
use crate::storage::MessageStore;

/// The gateway runtime manages all components and their lifecycle.
pub struct GatewayRuntime {
    config: AppConfig,
    bus: Arc<EventBus>,
    router: Arc<ApiRouter>,
    stats: Arc<RuntimeStats>,
    store: Option<Arc<MessageStore>>,
    dedup: Option<Arc<DedupFilter>>,
    adapters: Vec<Arc<dyn BackendAdapter>>,
    servers: Vec<Box<dyn ProtocolServer>>,
    /// Handle for the periodic stats refresher.
    stats_handle: Option<JoinHandle<()>>,
    /// Handle for the storage cleanup task.
    cleanup_handle: Option<JoinHandle<()>>,
    /// Handle for the storage persistence task.
    persist_handle: Option<JoinHandle<()>>,
}

impl GatewayRuntime {
    /// Create a new gateway runtime from the application config.
    pub fn new(config: AppConfig) -> Self {
        let bus = Arc::new(EventBus::new());
        let router = Arc::new(ApiRouter::new());

        // Optionally open message storage.
        let store = if config.storage.enabled {
            match MessageStore::open(&config.storage) {
                Ok(s) => {
                    info!(path = %config.storage.path, "message storage enabled");
                    Some(Arc::new(s))
                }
                Err(e) => {
                    error!("failed to open message store: {e}");
                    None
                }
            }
        } else {
            None
        };

        let stats = Arc::new(RuntimeStats::with_storage(store.is_some()));

        // Event deduplication filter — enabled by default, crucial for failover.
        let dedup = if config.dedup.enabled {
            info!(
                window_secs = config.dedup.window_secs,
                "event deduplication enabled"
            );
            Some(Arc::new(DedupFilter::new(config.dedup.window_secs)))
        } else {
            None
        };

        Self {
            config,
            bus,
            router,
            stats,
            store,
            dedup,
            adapters: Vec::new(),
            servers: Vec::new(),
            stats_handle: None,
            cleanup_handle: None,
            persist_handle: None,
        }
    }

    /// Register a backend adapter.
    pub fn add_adapter(&mut self, adapter: Arc<dyn BackendAdapter>) {
        self.adapters.push(adapter);
    }

    /// Register a protocol server.
    pub fn add_server(&mut self, server: Box<dyn ProtocolServer>) {
        self.servers.push(server);
    }

    /// Access the event bus.
    pub fn bus(&self) -> &Arc<EventBus> {
        &self.bus
    }

    /// Access the API router.
    pub fn router(&self) -> &Arc<ApiRouter> {
        &self.router
    }

    /// Access the runtime stats.
    pub fn stats(&self) -> &Arc<RuntimeStats> {
        &self.stats
    }

    /// Access the message store (if enabled).
    pub fn store(&self) -> &Option<Arc<MessageStore>> {
        &self.store
    }

    /// Access the dedup filter (if enabled).
    pub fn dedup(&self) -> &Option<Arc<DedupFilter>> {
        &self.dedup
    }

    /// Access the config.
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// Connect all registered adapters via the adapter manager and start
    /// background tasks (stats, persistence, cleanup).
    pub async fn start(&mut self, manager: &AdapterManager) -> Result<(), GatewayError> {
        info!(
            adapters = self.adapters.len(),
            servers = self.servers.len(),
            "starting ferroq gateway"
        );

        // Connect each adapter and register it with the adapter manager
        // (which handles forwarding, router registration, and lifecycle).
        for adapter in &self.adapters {
            let (event_tx, event_rx) = mpsc::unbounded_channel::<Event>();

            // Try to connect. If it fails, log a warning but don't abort —
            // the adapter has internal reconnect logic.
            match adapter.connect(event_tx).await {
                Ok(()) => {
                    info!(name = %adapter.info().name, "adapter connected");
                }
                Err(e) => {
                    warn!(
                        name = %adapter.info().name,
                        error = %e,
                        "adapter initial connection failed (will retry)"
                    );
                }
            }

            // Register the adapter with the API router.
            self.router.register(Arc::clone(adapter));

            // Hand off to the adapter manager for event forwarding and lifecycle tracking.
            manager.register_running(Arc::clone(adapter), event_rx);
        }

        // Spawn periodic stats refresher + health checker + dynamic self_id association (every 5s).
        // Uses the adapter manager to enumerate all adapters (including dynamically added ones).
        let adapters_for_stats: Vec<Arc<dyn BackendAdapter>> =
            self.adapters.iter().map(Arc::clone).collect();
        let stats_clone = Arc::clone(&self.stats);
        let router_clone = Arc::clone(&self.router);
        self.stats_handle = Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                interval.tick().await;
                let mut snapshots = Vec::with_capacity(adapters_for_stats.len());
                for (index, a) in adapters_for_stats.iter().enumerate() {
                    let info = a.info();

                    // Dynamic self_id → router association.
                    // If the adapter now knows its self_id, update the routing table.
                    if let Some(self_id) = info.self_id {
                        router_clone.associate_self_id(self_id, index);
                    }

                    let start = std::time::Instant::now();
                    let healthy = a.health_check().await;
                    let latency = start.elapsed().as_millis() as u64;
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    snapshots.push(AdapterSnapshot {
                        name: info.name,
                        backend_type: info.backend_type,
                        url: info.url,
                        state: info.state,
                        self_id: info.self_id,
                        healthy,
                        health_check_ms: Some(latency),
                        last_health_check: Some(now),
                        events_total: 0,
                        api_calls_total: 0,
                    });
                }
                stats_clone.update_adapters(snapshots);
            }
        }));

        // Do an initial snapshot immediately (health checks run in the periodic task).
        let snapshots: Vec<AdapterSnapshot> = self
            .adapters
            .iter()
            .map(|a| {
                let info = a.info();
                AdapterSnapshot {
                    name: info.name,
                    backend_type: info.backend_type,
                    url: info.url,
                    state: info.state,
                    self_id: info.self_id,
                    healthy: info.state == ferroq_core::adapter::AdapterState::Connected,
                    health_check_ms: None,
                    last_health_check: None,
                    events_total: 0,
                    api_calls_total: 0,
                }
            })
            .collect();
        self.stats.update_adapters(snapshots);

        // Spawn message persistence task if storage is enabled.
        if let Some(ref store) = self.store {
            let store_clone = Arc::clone(store);
            let stats_clone2 = Arc::clone(&self.stats);
            let mut persist_rx = self.bus.subscribe();
            self.persist_handle = Some(tokio::spawn(async move {
                let mut stored: u64 = 0;
                while let Ok(event) = persist_rx.recv().await {
                    if let Event::Message(ref msg) = event {
                        if let Err(e) = store_clone.insert(msg).await {
                            warn!("failed to persist message: {e}");
                        } else {
                            stored += 1;
                            stats_clone2.record_message_stored();
                            if stored % 1000 == 0 {
                                info!(total = stored, "message persistence progress");
                            }
                        }
                    }
                }
                warn!(total = stored, "message persistence task ended");
            }));

            // Spawn hourly cleanup.
            self.cleanup_handle = Some(crate::storage::spawn_cleanup_task(Arc::clone(store)));

            info!("message persistence task started");
        }

        info!("ferroq gateway started");
        Ok(())
    }

    /// Gracefully shut down background tasks.
    ///
    /// Adapter disconnection and forwarding abort are handled by
    /// `AdapterManager::shutdown()` — this method only stops the runtime's
    /// own background tasks (stats, persistence, cleanup).
    pub async fn shutdown(&mut self) -> Result<(), GatewayError> {
        info!("shutting down ferroq gateway");

        // Abort stats refresher.
        if let Some(h) = self.stats_handle.take() {
            h.abort();
        }

        // Abort storage tasks.
        if let Some(h) = self.persist_handle.take() {
            h.abort();
        }
        if let Some(h) = self.cleanup_handle.take() {
            h.abort();
        }

        info!("ferroq gateway shut down");
        Ok(())
    }
}
