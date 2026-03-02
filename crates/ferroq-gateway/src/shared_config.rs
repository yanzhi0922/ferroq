//! Runtime-mutable configuration shared across all gateway components.
//!
//! Updated atomically via the management `/api/reload` endpoint. Each field
//! is independently `RwLock`-protected so readers never block each other and
//! writers only briefly block the specific field being updated.

use parking_lot::RwLock;

/// Runtime-mutable configuration.
///
/// Created once at startup from [`AppConfig`] and then shared (via `Arc`) by
/// middleware, protocol servers, and the management API.
pub struct SharedConfig {
    access_token: RwLock<String>,
}

impl SharedConfig {
    /// Create from the initial access token.
    pub fn new(access_token: String) -> Self {
        Self {
            access_token: RwLock::new(access_token),
        }
    }

    /// Get the current access token.
    pub fn access_token(&self) -> String {
        self.access_token.read().clone()
    }

    /// Update the access token at runtime.
    pub fn set_access_token(&self, token: String) {
        *self.access_token.write() = token;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_and_set_access_token() {
        let cfg = SharedConfig::new("initial".into());
        assert_eq!(cfg.access_token(), "initial");

        cfg.set_access_token("updated".into());
        assert_eq!(cfg.access_token(), "updated");
    }

    #[test]
    fn empty_token_means_no_auth() {
        let cfg = SharedConfig::new(String::new());
        assert!(cfg.access_token().is_empty());
    }
}
