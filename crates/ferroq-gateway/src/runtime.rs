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

/// The gateway runtime manages all components and their lifecycle.
pub struct GatewayRuntime {
    config: AppConfig,
    bus: Arc<EventBus>,
    router: Arc<ApiRouter>,
    stats: Arc<RuntimeStats>,
    adapters: Vec<Arc<dyn BackendAdapter>>,
    servers: Vec<Box<dyn ProtocolServer>>,
    /// Handles for the event forwarding tasks (adapter → bus).
    forward_handles: Vec<JoinHandle<()>>,
    /// Handle for the periodic stats refresher.
    stats_handle: Option<JoinHandle<()>>,
}

impl GatewayRuntime {
    /// Create a new gateway runtime from the application config.
    pub fn new(config: AppConfig) -> Self {
        let bus = Arc::new(EventBus::new());
        let router = Arc::new(ApiRouter::new());
        let stats = Arc::new(RuntimeStats::new());

        Self {
            config,
            bus,
            router,
            stats,
            adapters: Vec::new(),
            servers: Vec::new(),
            forward_handles: Vec::new(),
            stats_handle: None,
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

        // Spawn periodic stats refresher (update adapter snapshots every 5s).
        let adapters_for_stats: Vec<Arc<dyn BackendAdapter>> =
            self.adapters.iter().map(Arc::clone).collect();
        let stats_clone = Arc::clone(&self.stats);
        self.stats_handle = Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                interval.tick().await;
                let snapshots: Vec<AdapterSnapshot> = adapters_for_stats
                    .iter()
                    .map(|a| {
                        let info = a.info();
                        AdapterSnapshot {
                            name: info.name,
                            backend_type: info.backend_type,
                            url: info.url,
                            state: info.state,
                            self_id: info.self_id,
                        }
                    })
                    .collect();
                stats_clone.update_adapters(snapshots);
            }
        }));

        // Do an initial snapshot immediately.
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
                }
            })
            .collect();
        self.stats.update_adapters(snapshots);

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

        info!("ferroq gateway shut down");
        Ok(())
    }
}
