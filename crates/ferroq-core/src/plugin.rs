//! Plugin interface for WASM plugins.
//!
//! This module defines the types and traits for the WASM plugin system.
//! Plugins can intercept events and API calls, allowing for:
//! - Message filtering and transformation
//! - Auto-reply functionality
//! - Rate limiting
//! - Custom logging and analytics

use serde::{Deserialize, Serialize};

/// Result of a plugin operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginResult {
    /// Continue processing (pass to next plugin / actual handler).
    Continue,

    /// Event/call was handled; stop processing and don't pass to next handler.
    Handled,

    /// Drop this event/call entirely (don't even send a response).
    Drop,

    /// An error occurred in the plugin.
    Error(String),
}

/// Plugin metadata returned by the plugin's `info` function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Plugin name.
    pub name: String,

    /// Plugin version (semver).
    pub version: String,

    /// Brief description.
    pub description: String,

    /// Author name or organization.
    pub author: String,
}

impl Default for PluginInfo {
    fn default() -> Self {
        Self {
            name: "unknown".to_string(),
            version: "0.0.0".to_string(),
            description: String::new(),
            author: String::new(),
        }
    }
}

/// Plugin hook type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginHook {
    /// Called when an event is received from a backend.
    OnEvent,

    /// Called when an API request is received from an upstream client.
    OnApiCall,

    /// Called when an API response is about to be sent.
    OnApiResponse,
}

/// Plugin configuration passed during initialization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginRuntimeConfig {
    /// Plugin-specific configuration as JSON.
    #[serde(default)]
    pub config: serde_json::Value,
}

// ----- WASM ABI: Canonical ABI for string passing -----
//
// Plugins export these functions:
//
// ```
// // Required: Plugin info
// fn ferroq_plugin_info() -> *const u8;  // Returns JSON-encoded PluginInfo
//
// // Optional: Called on load with config
// fn ferroq_plugin_init(config_ptr: *const u8, config_len: u32) -> i32;
//
// // Optional: Called on event (JSON-encoded Event)
// fn ferroq_on_event(event_ptr: *const u8, event_len: u32) -> i32;
// // Returns: PluginResult as i32 (0=Continue, 1=Handled, 2=Drop, -1=Error)
// // If event was mutated, plugin should write result via ferroq_set_result
//
// // Optional: Called on API call (JSON-encoded ApiRequest)
// fn ferroq_on_api_call(req_ptr: *const u8, req_len: u32) -> i32;
//
// // Host-provided: Write result back
// // Plugin calls: ferroq_set_result(ptr: *const u8, len: u32)
// ```
//
// For simplicity, we use JSON encoding for all data crossing the WASM boundary.
// This is not the most efficient but makes debugging and plugin development easier.

/// WASM function export names.
pub mod wasm_exports {
    /// Plugin info function name.
    pub const PLUGIN_INFO: &str = "ferroq_plugin_info";

    /// Plugin init function name.
    pub const PLUGIN_INIT: &str = "ferroq_plugin_init";

    /// On-event hook function name.
    pub const ON_EVENT: &str = "ferroq_on_event";

    /// On-API-call hook function name.
    pub const ON_API_CALL: &str = "ferroq_on_api_call";

    /// Memory allocation function (plugin must export).
    pub const ALLOC: &str = "ferroq_alloc";

    /// Memory deallocation function (plugin must export).
    pub const DEALLOC: &str = "ferroq_dealloc";
}

/// Convert PluginResult to/from i32 for WASM boundary.
impl PluginResult {
    /// Convert to i32 for WASM return value.
    pub fn to_i32(&self) -> i32 {
        match self {
            PluginResult::Continue => 0,
            PluginResult::Handled => 1,
            PluginResult::Drop => 2,
            PluginResult::Error(_) => -1,
        }
    }

    /// Convert from i32 WASM return value.
    pub fn from_i32(code: i32) -> Self {
        match code {
            0 => PluginResult::Continue,
            1 => PluginResult::Handled,
            2 => PluginResult::Drop,
            _ => PluginResult::Error(format!("unknown result code: {code}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_result_round_trip() {
        assert_eq!(
            PluginResult::from_i32(PluginResult::Continue.to_i32()),
            PluginResult::Continue
        );
        assert_eq!(
            PluginResult::from_i32(PluginResult::Handled.to_i32()),
            PluginResult::Handled
        );
        assert_eq!(
            PluginResult::from_i32(PluginResult::Drop.to_i32()),
            PluginResult::Drop
        );
    }

    #[test]
    fn plugin_info_default() {
        let info = PluginInfo::default();
        assert_eq!(info.name, "unknown");
        assert_eq!(info.version, "0.0.0");
    }

    #[test]
    fn plugin_info_serialize() {
        let info = PluginInfo {
            name: "echo".to_string(),
            version: "1.0.0".to_string(),
            description: "Echo plugin".to_string(),
            author: "ferroq".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("echo"));
        assert!(json.contains("1.0.0"));
    }
}
