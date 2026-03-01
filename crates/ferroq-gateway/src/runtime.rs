//! Gateway runtime — orchestrates adapters, event bus, and protocol servers.

use std::sync::Arc;

use ferroq_core::adapter::BackendAdapter;
use ferroq_core::config::AppConfig;
use ferroq_core::error::GatewayError;
use ferroq_core::protocol::ProtocolServer;
use tracing::info;

use crate::bus::EventBus;
use crate::router::ApiRouter;

/// The gateway runtime manages all components and their lifecycle.
pub struct GatewayRuntime {
    config: AppConfig,
    bus: Arc<EventBus>,
    router: Arc<ApiRouter>,
    adapters: Vec<Box<dyn BackendAdapter>>,
    servers: Vec<Box<dyn ProtocolServer>>,
}

impl GatewayRuntime {
    /// Create a new gateway runtime from the application config.
    pub fn new(config: AppConfig) -> Self {
        let bus = Arc::new(EventBus::new());
        let router = Arc::new(ApiRouter::new());

        Self {
            config,
            bus,
            router,
            adapters: Vec::new(),
            servers: Vec::new(),
        }
    }

    /// Register a backend adapter.
    pub fn add_adapter(&mut self, adapter: Box<dyn BackendAdapter>) {
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

    /// Access the config.
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// Start all adapters and protocol servers.
    ///
    /// This is a placeholder for the full lifecycle management that will be
    /// implemented in later phases.
    pub async fn start(&mut self) -> Result<(), GatewayError> {
        info!(
            accounts = self.config.accounts.len(),
            servers = self.servers.len(),
            "starting ferroq gateway"
        );

        // TODO: Phase 1.3 — connect adapters, spawn event forwarding tasks
        // TODO: Phase 1.4 — start protocol servers

        info!("ferroq gateway started (no adapters connected yet — Phase 1 skeleton)");
        Ok(())
    }

    /// Gracefully shut down all components.
    pub async fn shutdown(&mut self) -> Result<(), GatewayError> {
        info!("shutting down ferroq gateway");
        // TODO: disconnect adapters, stop servers
        Ok(())
    }
}
