//! Backend adapter trait.
//!
//! Implementors connect to a downstream QQ protocol backend
//! (Lagrange, NapCat, Official API, etc.) and normalize events.

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::api::{ApiRequest, ApiResponse};
use crate::error::GatewayError;
use crate::event::Event;

/// Connection state of a backend adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum AdapterState {
    /// Not yet connected.
    Disconnected,
    /// Connection in progress.
    Connecting,
    /// Connected and healthy.
    Connected,
    /// Connection lost, attempting to reconnect.
    Reconnecting,
    /// Permanently failed (e.g. invalid config).
    Failed,
}

impl std::fmt::Display for AdapterState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disconnected => write!(f, "disconnected"),
            Self::Connecting => write!(f, "connecting"),
            Self::Connected => write!(f, "connected"),
            Self::Reconnecting => write!(f, "reconnecting"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// Information about a backend adapter.
#[derive(Debug, Clone)]
pub struct AdapterInfo {
    /// Human-readable name for this adapter instance.
    pub name: String,
    /// Backend type: "lagrange", "napcat", "official", "mock".
    pub backend_type: String,
    /// The URL this adapter connects to.
    pub url: String,
    /// Current connection state.
    pub state: AdapterState,
    /// The self_id (QQ number) reported by the backend, if known.
    pub self_id: Option<i64>,
}

/// Trait for downstream backend adapters.
///
/// Each adapter manages a connection to one backend instance and exposes
/// a unified event stream + API call interface.
///
/// All methods take `&self` — implementations use interior mutability
/// so that adapters can be shared via `Arc<dyn BackendAdapter>`.
#[async_trait]
pub trait BackendAdapter: Send + Sync + 'static {
    /// Returns metadata about this adapter.
    fn info(&self) -> AdapterInfo;

    /// Connect to the backend and start the internal read/write loops.
    ///
    /// Events will be sent to the provided `event_tx` channel.
    /// This method returns once the initial connection is established
    /// (or fails). Reconnection is handled internally.
    async fn connect(&self, event_tx: mpsc::UnboundedSender<Event>) -> Result<(), GatewayError>;

    /// Disconnect from the backend gracefully.
    async fn disconnect(&self) -> Result<(), GatewayError>;

    /// Send an API call to the backend and wait for the response.
    async fn call_api(&self, request: ApiRequest) -> Result<ApiResponse, GatewayError>;

    /// Perform a health check. Returns `true` if the backend is responsive.
    async fn health_check(&self) -> bool;
}
