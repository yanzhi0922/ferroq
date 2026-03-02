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

use crate::bus::EventBus;
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
    adapters: Vec<Arc<dyn BackendAdapter>>,
    servers: Vec<Box<dyn ProtocolServer>>,
    /// Handles for the event forwarding tasks (adapter → bus).
    forward_handles: Vec<JoinHandle<()>>,
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

        Self {
            config,
            bus,
            router,
            stats,
            store,
            adapters: Vec::new(),
            servers: Vec::new(),
            forward_handles: Vec::new(),
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

    /// Access the config.
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// Connect all registered adapters and start forwarding events to the bus.
    pub async fn start(&mut self) -> Result<(), GatewayError> {
        info!(
            adapters = self.adapters.len(),
            servers = self.servers.len(),
            "starting ferroq gateway"
        );

        // Connect each adapter and register it with the router.
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

            // Spawn a task that forwards events from this adapter to the bus.
            let bus = Arc::clone(&self.bus);
            let stats = Arc::clone(&self.stats);
            let adapter_name = adapter.info().name.clone();
            let handle = tokio::spawn(Self::forward_events(event_rx, bus, stats, adapter_name));
            self.forward_handles.push(handle);
        }

        // Spawn periodic stats refresher + health checker (every 5s).
        let adapters_for_stats: Vec<Arc<dyn BackendAdapter>> =
            self.adapters.iter().map(Arc::clone).collect();
        let stats_clone = Arc::clone(&self.stats);
        self.stats_handle = Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                interval.tick().await;
                let mut snapshots = Vec::with_capacity(adapters_for_stats.len());
                for a in &adapters_for_stats {
                    let info = a.info();
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
            self.cleanup_handle =
                Some(crate::storage::spawn_cleanup_task(Arc::clone(store)));

            info!("message persistence task started");
        }

        info!("ferroq gateway started");
        Ok(())
    }

    /// Forward events from an adapter's channel to the event bus.
    async fn forward_events(
        mut event_rx: mpsc::UnboundedReceiver<Event>,
        bus: Arc<EventBus>,
        stats: Arc<RuntimeStats>,
        adapter_name: String,
    ) {
        let mut count: u64 = 0;
        while let Some(event) = event_rx.recv().await {
            count += 1;
            stats.record_event();
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

    /// Gracefully shut down all components.
    pub async fn shutdown(&mut self) -> Result<(), GatewayError> {
        info!("shutting down ferroq gateway");

        // Disconnect all adapters.
        for adapter in &self.adapters {
            if let Err(e) = adapter.disconnect().await {
                error!(name = %adapter.info().name, error = %e, "error disconnecting adapter");
            }
        }

        // Abort forwarding tasks.
        for handle in self.forward_handles.drain(..) {
            handle.abort();
        }

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
