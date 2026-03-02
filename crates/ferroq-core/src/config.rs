//! Configuration types.
//!
//! Deserialized from `config.yaml` at startup.

use serde::{Deserialize, Serialize};

/// Top-level configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Server settings.
    #[serde(default)]
    pub server: ServerConfig,

    /// Account + backend definitions.
    #[serde(default)]
    pub accounts: Vec<AccountConfig>,

    /// Inbound protocol settings.
    #[serde(default)]
    pub protocols: ProtocolsConfig,

    /// Message storage settings.
    #[serde(default)]
    pub storage: StorageConfig,

    /// Event deduplication settings.
    #[serde(default)]
    pub dedup: DedupConfig,

    /// Logging settings.
    #[serde(default)]
    pub logging: LoggingConfig,
}

/// Server settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port")]
    pub port: u16,

    /// Global access token for API authentication.
    #[serde(default)]
    pub access_token: String,

    /// Enable the web dashboard.
    #[serde(default = "default_true")]
    pub dashboard: bool,

    /// Rate limiting settings.
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            access_token: String::new(),
            dashboard: true,
            rate_limit: RateLimitConfig::default(),
        }
    }
}

/// Rate limiting configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Whether rate limiting is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Maximum requests per second (global).
    #[serde(default = "default_rps")]
    pub requests_per_second: u32,

    /// Burst size — how many requests can arrive at once before throttling.
    #[serde(default = "default_burst")]
    pub burst: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            requests_per_second: default_rps(),
            burst: default_burst(),
        }
    }
}

/// Account configuration — each account binds to one backend (with optional fallback).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    /// Human-readable account name.
    pub name: String,

    /// Primary backend connection.
    pub backend: BackendConfig,

    /// Optional fallback backend (for failover).
    #[serde(default)]
    pub fallback: Option<BackendConfig>,
}

/// Backend connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    /// Backend type: "lagrange", "napcat", "official", "mock".
    #[serde(rename = "type")]
    pub backend_type: String,

    /// WebSocket or HTTP URL to connect to the backend.
    pub url: String,

    /// Access token for the backend connection.
    #[serde(default)]
    pub access_token: String,

    /// Base reconnect interval in seconds (used as minimum for exponential backoff).
    #[serde(default = "default_reconnect_interval")]
    pub reconnect_interval: u64,

    /// Maximum reconnect interval in seconds (backoff cap).
    #[serde(default = "default_max_reconnect_interval")]
    pub max_reconnect_interval: u64,

    /// Health check interval in seconds.
    #[serde(default = "default_health_check_interval")]
    pub health_check_interval: u64,

    /// WebSocket connect timeout in seconds.
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout: u64,

    /// API call response timeout in seconds.
    #[serde(default = "default_api_timeout")]
    pub api_timeout: u64,
}

/// Protocol output configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProtocolsConfig {
    #[serde(default)]
    pub onebot_v11: Option<OneBotV11Config>,

    #[serde(default)]
    pub onebot_v12: Option<OneBotV12Config>,

    #[serde(default)]
    pub milky: Option<MilkyConfig>,

    #[serde(default)]
    pub satori: Option<SatoriConfig>,
}

/// OneBot v11 specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OneBotV11Config {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_true")]
    pub http: bool,

    #[serde(default = "default_true")]
    pub ws: bool,

    #[serde(default)]
    pub ws_reverse: Vec<WsReverseTarget>,

    #[serde(default)]
    pub http_post: Vec<HttpPostTarget>,
}

/// A reverse WebSocket target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsReverseTarget {
    pub url: String,
    #[serde(default)]
    pub access_token: String,
}

/// An HTTP POST event target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpPostTarget {
    pub url: String,
    #[serde(default)]
    pub secret: String,
}

/// OneBot v12 specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OneBotV12Config {
    #[serde(default)]
    pub enabled: bool,

    /// Enable HTTP action endpoint.
    #[serde(default = "default_true")]
    pub http: bool,

    /// Enable forward WebSocket endpoint.
    #[serde(default = "default_true")]
    pub ws: bool,
}

/// Placeholder for Milky config (future).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilkyConfig {
    #[serde(default)]
    pub enabled: bool,
}

/// Satori protocol configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SatoriConfig {
    #[serde(default)]
    pub enabled: bool,

    /// Enable HTTP API endpoint (`POST /v1/{resource}.{method}`).
    #[serde(default = "default_true")]
    pub http: bool,

    /// Enable WebSocket event endpoint (`/v1/events`).
    #[serde(default = "default_true")]
    pub ws: bool,
}

/// Message storage settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_storage_path")]
    pub path: String,

    #[serde(default = "default_max_days")]
    pub max_days: u32,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            path: default_storage_path(),
            max_days: default_max_days(),
        }
    }
}

/// Event deduplication settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DedupConfig {
    /// Whether deduplication is enabled (recommended when using failover).
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Time window in seconds — events with the same fingerprint within this
    /// window are considered duplicates.
    #[serde(default = "default_dedup_window")]
    pub window_secs: u64,
}

impl Default for DedupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            window_secs: default_dedup_window(),
        }
    }
}

/// Logging settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,

    #[serde(default)]
    pub file: Option<String>,

    #[serde(default = "default_true")]
    pub console: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            file: None,
            console: true,
        }
    }
}

// --- Default value helpers ---

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_true() -> bool {
    true
}

fn default_reconnect_interval() -> u64 {
    5
}

fn default_max_reconnect_interval() -> u64 {
    120
}

fn default_health_check_interval() -> u64 {
    30
}

fn default_connect_timeout() -> u64 {
    15
}

fn default_api_timeout() -> u64 {
    30
}

fn default_storage_path() -> String {
    "./data/messages.db".to_string()
}

fn default_max_days() -> u32 {
    30
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_dedup_window() -> u64 {
    60
}

fn default_rps() -> u32 {
    100
}

fn default_burst() -> u32 {
    200
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_minimal_config() {
        let yaml = r#"
server:
  port: 9090
accounts:
  - name: "test"
    backend:
      type: mock
      url: "ws://localhost:8081"
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).expect("parse yaml");
        assert_eq!(config.server.port, 9090);
        assert_eq!(config.accounts.len(), 1);
        assert_eq!(config.accounts[0].backend.backend_type, "mock");
    }

    #[test]
    fn default_server_config() {
        let config = ServerConfig::default();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert!(config.dashboard);
    }

    #[test]
    fn backend_config_defaults() {
        let yaml = r#"
type: lagrange
url: "ws://localhost:8081"
"#;
        let cfg: BackendConfig = serde_yaml::from_str(yaml).expect("parse backend config");
        assert_eq!(cfg.reconnect_interval, 5);
        assert_eq!(cfg.max_reconnect_interval, 120);
        assert_eq!(cfg.health_check_interval, 30);
        assert_eq!(cfg.connect_timeout, 15);
        assert_eq!(cfg.api_timeout, 30);
    }

    #[test]
    fn backend_config_custom_timeouts() {
        let yaml = r#"
type: lagrange
url: "ws://localhost:8081"
connect_timeout: 10
api_timeout: 60
max_reconnect_interval: 300
"#;
        let cfg: BackendConfig = serde_yaml::from_str(yaml).expect("parse backend config");
        assert_eq!(cfg.connect_timeout, 10);
        assert_eq!(cfg.api_timeout, 60);
        assert_eq!(cfg.max_reconnect_interval, 300);
    }
}
