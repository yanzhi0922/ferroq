//! Dynamic adapter manager.
//!
//! `AdapterManager` provides runtime adapter lifecycle management — adding,
//! removing, and reconnecting backend adapters without restarting the gateway.
//!
//! It holds shared references to the bus, router, stats, and dedup filter,
//! and manages the event-forwarding tasks spawned for each adapter.

use std::collections::HashMap;
use std::sync::Arc;

use ferroq_core::adapter::BackendAdapter;
use ferroq_core::config::BackendConfig;
use ferroq_core::error::GatewayError;
use ferroq_core::event::Event;
use parking_lot::RwLock;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::adapter::LagrangeAdapter;
use crate::bus::EventBus;
use crate::dedup::DedupFilter;
use crate::forward;
use crate::router::ApiRouter;
use crate::stats::RuntimeStats;

/// Tracks a running adapter and its forwarding task.
struct LiveAdapter {
    adapter: Arc<dyn BackendAdapter>,
    forward_handle: JoinHandle<()>,
}

/// Manages the lifecycle of backend adapters at runtime.
pub struct AdapterManager {
    bus: Arc<EventBus>,
    router: Arc<ApiRouter>,
    stats: Arc<RuntimeStats>,
    dedup: Option<Arc<DedupFilter>>,
    /// Map from adapter name → live adapter state.
    live: RwLock<HashMap<String, LiveAdapter>>,
}

impl AdapterManager {
    /// Create a new adapter manager with shared gateway components.
    pub fn new(
        bus: Arc<EventBus>,
        router: Arc<ApiRouter>,
        stats: Arc<RuntimeStats>,
        dedup: Option<Arc<DedupFilter>>,
    ) -> Self {
        Self {
            bus,
            router,
            stats,
            dedup,
            live: RwLock::new(HashMap::new()),
        }
    }

    /// Register an already-connected adapter and start its forwarding task.
    ///
    /// Called during startup for each adapter created from config so that the
    /// adapter manager is aware of all adapters (not just dynamically added ones).
    pub fn register_running(
        &self,
        adapter: Arc<dyn BackendAdapter>,
        event_rx: mpsc::UnboundedReceiver<Event>,
    ) {
        let name = adapter.info().name.clone();

        // Spawn event forwarding.
        let handle = self.spawn_forwarder(&name, event_rx);

        self.live.write().insert(
            name,
            LiveAdapter {
                adapter,
                forward_handle: handle,
            },
        );
    }

    /// Dynamically add a new adapter from a backend config.
    ///
    /// Creates the adapter, connects it, registers it, and starts forwarding.
    pub async fn add_adapter(
        &self,
        name: &str,
        config: &BackendConfig,
    ) -> Result<(), GatewayError> {
        // Check for duplicate name.
        if self.live.read().contains_key(name) {
            return Err(GatewayError::Internal(format!(
                "adapter '{}' already exists",
                name
            )));
        }

        // Create the adapter based on backend type.
        let adapter: Arc<dyn BackendAdapter> = match config.backend_type.as_str() {
            "lagrange" | "napcat" => {
                let a = LagrangeAdapter::from_backend_config(name, config);
                Arc::new(a)
            }
            other => {
                return Err(GatewayError::Internal(format!(
                    "unknown backend type: {}",
                    other
                )));
            }
        };

        // Connect.
        let (event_tx, event_rx) = mpsc::unbounded_channel::<Event>();
        match adapter.connect(event_tx).await {
            Ok(()) => {
                info!(name, "dynamically added adapter connected");
            }
            Err(e) => {
                warn!(name, error = %e, "adapter connect failed (will retry internally)");
            }
        }

        // Register with router.
        self.router.register(Arc::clone(&adapter));

        // Spawn forwarder.
        let handle = self.spawn_forwarder(name, event_rx);

        self.live.write().insert(
            name.to_string(),
            LiveAdapter {
                adapter,
                forward_handle: handle,
            },
        );

        info!(name, "adapter added dynamically");
        Ok(())
    }

    /// Remove an adapter by name.
    ///
    /// Disconnects the adapter, aborts its forwarding task, and unregisters
    /// it from the router.
    pub async fn remove_adapter(&self, name: &str) -> Result<(), GatewayError> {
        let live = self.live.write().remove(name);
        let Some(entry) = live else {
            return Err(GatewayError::Internal(format!(
                "adapter '{}' not found",
                name
            )));
        };

        // Abort the forwarding task.
        entry.forward_handle.abort();

        // Disconnect the adapter.
        if let Err(e) = entry.adapter.disconnect().await {
            error!(name, error = %e, "error disconnecting removed adapter");
        }

        // Remove from router.
        self.router.unregister(name);

        info!(name, "adapter removed");
        Ok(())
    }

    /// Reconnect a specific adapter.
    ///
    /// Disconnects, then reconnects with a fresh event channel.
    pub async fn reconnect_adapter(&self, name: &str) -> Result<(), GatewayError> {
        // Extract the adapter Arc and abort the old forwarder without holding
        // the lock across awaits (parking_lot guards are !Send).
        let adapter = {
            let mut live = self.live.write();
            let entry = live
                .get_mut(name)
                .ok_or_else(|| GatewayError::Internal(format!("adapter '{}' not found", name)))?;

            // Abort old forwarder.
            entry.forward_handle.abort();

            Arc::clone(&entry.adapter)
        };
        // Lock released here.

        // Disconnect.
        if let Err(e) = adapter.disconnect().await {
            warn!(name, error = %e, "disconnect error during reconnect (continuing)");
        }

        // Reconnect with a new event channel.
        let (event_tx, event_rx) = mpsc::unbounded_channel::<Event>();
        match adapter.connect(event_tx).await {
            Ok(()) => {
                info!(name, "adapter reconnected");
            }
            Err(e) => {
                warn!(name, error = %e, "reconnect failed (adapter will retry internally)");
            }
        }

        // Spawn a new forwarder and update the live entry.
        let handle = self.spawn_forwarder(name, event_rx);
        {
            let mut live = self.live.write();
            if let Some(entry) = live.get_mut(name) {
                entry.forward_handle = handle;
            }
        }

        Ok(())
    }

    /// List all adapter names.
    pub fn list_names(&self) -> Vec<String> {
        self.live.read().keys().cloned().collect()
    }

    /// Return cloned `Arc` references to all live adapters.
    pub fn adapters(&self) -> Vec<Arc<dyn BackendAdapter>> {
        self.live
            .read()
            .values()
            .map(|la| Arc::clone(&la.adapter))
            .collect()
    }

    /// Check if an adapter with the given name exists.
    pub fn has(&self, name: &str) -> bool {
        self.live.read().contains_key(name)
    }

    /// Disconnect all adapters and abort all forwarding tasks.
    pub async fn shutdown(&self) {
        let entries: Vec<(String, LiveAdapter)> = self.live.write().drain().collect();
        for (name, entry) in entries {
            entry.forward_handle.abort();
            if let Err(e) = entry.adapter.disconnect().await {
                error!(name = %name, error = %e, "error disconnecting during shutdown");
            }
        }
    }

    /// Spawn an event-forwarding task for the named adapter.
    fn spawn_forwarder(
        &self,
        name: &str,
        event_rx: mpsc::UnboundedReceiver<Event>,
    ) -> JoinHandle<()> {
        let bus = Arc::clone(&self.bus);
        let stats = Arc::clone(&self.stats);
        let dedup = self.dedup.clone();
        let adapter_name = name.to_string();
        tokio::spawn(forward::forward_events(
            event_rx,
            bus,
            stats,
            dedup,
            adapter_name,
        ))
    }
}
