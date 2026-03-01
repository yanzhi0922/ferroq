//! Backend adapter trait.
//!
//! Implementors connect to a downstream QQ protocol backend
//! (Lagrange, NapCat, Official API, etc.) and normalize events.

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

use crate::api::{ApiRequest, ApiResponse};
use crate::error::GatewayError;
use crate::event::Event;

/// Connection state of a backend adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[async_trait]
pub trait BackendAdapter: Send + Sync + 'static {
    /// Returns metadata about this adapter.
    fn info(&self) -> AdapterInfo;

    /// Connect to the backend. Returns when the connection is established
    /// or an error occurs.
    async fn connect(&mut self) -> Result<(), GatewayError>;

    /// Disconnect from the backend gracefully.
    async fn disconnect(&mut self) -> Result<(), GatewayError>;

    /// Send an API call to the backend and wait for the response.
    async fn call_api(&self, request: ApiRequest) -> Result<ApiResponse, GatewayError>;

    /// Returns a stream of events from the backend.
    ///
    /// The stream should yield events as they arrive. If the connection is
    /// lost, the stream should end (not error), and the caller will handle
    /// reconnection.
    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>>;

    /// Perform a health check. Returns `true` if the backend is responsive.
    async fn health_check(&self) -> bool;
}
