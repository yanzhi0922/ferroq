//! Protocol server trait.
//!
//! Implementors expose an inbound protocol (OneBot v11, v12, Milky, Satori)
//! to upstream bot frameworks.

use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::api::{ApiRequest, ApiResponse};
use crate::error::GatewayError;
use crate::event::Event;

/// A callback used by protocol servers to route API requests to the backend.
pub type ApiRouter = Box<dyn Fn(ApiRequest) -> futures::future::BoxFuture<'static, Result<ApiResponse, GatewayError>> + Send + Sync>;

/// Trait for inbound protocol servers.
///
/// Each protocol server listens for connections from upstream clients (bot
/// frameworks like NoneBot, Koishi, etc.) and translates between the protocol
/// format and the internal event/API types.
#[async_trait]
pub trait ProtocolServer: Send + Sync + 'static {
    /// Protocol name, e.g. "onebot_v11", "milky", "satori".
    fn name(&self) -> &str;

    /// Start the protocol server.
    ///
    /// - `event_rx`: subscribe to the event bus to push events to clients.
    /// - `api_router`: callback to route API requests to backend adapters.
    ///
    /// This method should run until the server is shut down.
    async fn start(
        &self,
        event_rx: broadcast::Receiver<Event>,
        api_router: ApiRouter,
    ) -> Result<(), GatewayError>;

    /// Gracefully shut down the protocol server.
    async fn shutdown(&self) -> Result<(), GatewayError>;
}
