//! Unified API request / response types.
//!
//! These mirror the OneBot v11 action API format but are backend-agnostic.

use serde::{Deserialize, Serialize};

/// An API request from an upstream client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequest {
    /// Action name, e.g. "send_group_msg", "get_login_info".
    pub action: String,

    /// Parameters for the action.
    #[serde(default)]
    pub params: serde_json::Value,

    /// Optional echo field for request tracking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub echo: Option<serde_json::Value>,

    /// Which account (self_id) this request targets.
    /// If None, the gateway routes to the default/first account.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_id: Option<i64>,
}

/// An API response to return to the upstream client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    /// Status: "ok" or "failed".
    pub status: String,

    /// Return code: 0 for success, non-zero for error.
    pub retcode: i32,

    /// Response data.
    pub data: serde_json::Value,

    /// Human-readable message (usually empty on success).
    #[serde(default)]
    pub message: String,

    /// Echo back the request's echo field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub echo: Option<serde_json::Value>,
}

impl ApiResponse {
    /// Create a successful response.
    pub fn ok(data: serde_json::Value) -> Self {
        Self {
            status: "ok".to_string(),
            retcode: 0,
            data,
            message: String::new(),
            echo: None,
        }
    }

    /// Create a failed response.
    pub fn fail(retcode: i32, message: impl Into<String>) -> Self {
        Self {
            status: "failed".to_string(),
            retcode,
            data: serde_json::Value::Null,
            message: message.into(),
            echo: None,
        }
    }

    /// Attach the echo field from the originating request.
    pub fn with_echo(mut self, echo: Option<serde_json::Value>) -> Self {
        self.echo = echo;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_response_ok() {
        let resp = ApiResponse::ok(serde_json::json!({"message_id": 12345}));
        assert_eq!(resp.status, "ok");
        assert_eq!(resp.retcode, 0);
    }

    #[test]
    fn api_response_fail() {
        let resp = ApiResponse::fail(1400, "bad request");
        assert_eq!(resp.status, "failed");
        assert_eq!(resp.retcode, 1400);
        assert_eq!(resp.message, "bad request");
    }
}
