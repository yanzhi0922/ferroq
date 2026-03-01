//! Error types for ferroq.

use thiserror::Error;

/// Top-level gateway error type.
#[derive(Debug, Error)]
pub enum GatewayError {
    /// Backend connection failed.
    #[error("backend connection error: {0}")]
    Connection(String),

    /// Backend returned an error response.
    #[error("backend API error: {action} returned code {retcode}: {message}")]
    BackendApi {
        action: String,
        retcode: i32,
        message: String,
    },

    /// Configuration error.
    #[error("configuration error: {0}")]
    Config(String),

    /// Serialization / deserialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// WebSocket error.
    #[error("websocket error: {0}")]
    WebSocket(String),

    /// HTTP error.
    #[error("http error: {0}")]
    Http(String),

    /// Storage error.
    #[error("storage error: {0}")]
    Storage(String),

    /// Plugin error.
    #[error("plugin error: {0}")]
    Plugin(String),

    /// Authentication error.
    #[error("authentication error: {0}")]
    Auth(String),

    /// The requested account was not found.
    #[error("account not found: {0}")]
    AccountNotFound(String),

    /// Internal error.
    #[error("internal error: {0}")]
    Internal(String),
}
