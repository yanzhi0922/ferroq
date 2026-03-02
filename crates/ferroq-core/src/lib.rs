//! # ferroq-core
//!
//! Core types, traits, and abstractions for the ferroq QQ Bot unified gateway.
//!
//! This crate contains no I/O logic — only data structures, error types, and trait
//! definitions that the rest of the workspace depends on.

pub mod adapter;
pub mod api;
pub mod config;
pub mod error;
pub mod event;
pub mod message;
pub mod plugin;
pub mod protocol;
pub mod validation;

/// Re-export key types for convenience.
pub use adapter::{AdapterInfo, AdapterState, BackendAdapter};
pub use api::{ApiRequest, ApiResponse};
pub use config::AppConfig;
pub use error::GatewayError;
pub use event::Event;
pub use plugin::{PluginInfo, PluginResult};
