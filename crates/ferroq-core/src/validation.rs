//! Configuration validation.
//!
//! Validates the loaded [`AppConfig`] at startup before any I/O begins.
//! Returns all detected issues (not just the first one) so the user can fix
//! everything in a single pass.

use crate::config::{AppConfig, BackendConfig, OneBotV11Config};

/// A validation issue with severity level.
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub path: String,
    pub message: String,
}

/// Severity level of a validation issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The configuration is invalid and will prevent startup.
    Error,
    /// The configuration is suspicious but may work.
    Warning,
}

impl std::fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let level = match self.severity {
            Severity::Error => "ERROR",
            Severity::Warning => "WARN",
        };
        write!(f, "[{}] {}: {}", level, self.path, self.message)
    }
}

/// Validate the full application config. Returns a list of issues (may be empty).
pub fn validate(config: &AppConfig) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    validate_server(config, &mut issues);
    validate_accounts(config, &mut issues);
    validate_protocols(config, &mut issues);

    issues
}

/// Returns `true` if the config has any errors (not just warnings).
pub fn has_errors(issues: &[ValidationIssue]) -> bool {
    issues.iter().any(|i| i.severity == Severity::Error)
}

fn validate_server(config: &AppConfig, issues: &mut Vec<ValidationIssue>) {
    if config.server.port == 0 {
        issues.push(ValidationIssue {
            severity: Severity::Error,
            path: "server.port".into(),
            message: "port must be > 0".into(),
        });
    }

    if config.server.host.is_empty() {
        issues.push(ValidationIssue {
            severity: Severity::Error,
            path: "server.host".into(),
            message: "host must not be empty".into(),
        });
    }
}

fn validate_accounts(config: &AppConfig, issues: &mut Vec<ValidationIssue>) {
    if config.accounts.is_empty() {
        issues.push(ValidationIssue {
            severity: Severity::Warning,
            path: "accounts".into(),
            message: "no accounts configured — the gateway will have no backends".into(),
        });
        return;
    }

    let mut seen_names = std::collections::HashSet::new();
    for (i, account) in config.accounts.iter().enumerate() {
        let prefix = format!("accounts[{}]", i);

        // Name must be non-empty.
        if account.name.trim().is_empty() {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                path: format!("{prefix}.name"),
                message: "account name must not be empty".into(),
            });
        }

        // Name must be unique.
        if !seen_names.insert(&account.name) {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                path: format!("{prefix}.name"),
                message: format!("duplicate account name: \"{}\"", account.name),
            });
        }

        validate_backend(&account.backend, &format!("{prefix}.backend"), issues);

        if let Some(ref fallback) = account.fallback {
            validate_backend(fallback, &format!("{prefix}.fallback"), issues);
        }
    }
}

fn validate_backend(backend: &BackendConfig, path: &str, issues: &mut Vec<ValidationIssue>) {
    let known_types = ["lagrange", "napcat", "official", "mock"];
    if !known_types.contains(&backend.backend_type.as_str()) {
        issues.push(ValidationIssue {
            severity: Severity::Warning,
            path: format!("{path}.type"),
            message: format!(
                "unknown backend type \"{}\". Known types: {}",
                backend.backend_type,
                known_types.join(", ")
            ),
        });
    }

    if backend.url.is_empty() {
        issues.push(ValidationIssue {
            severity: Severity::Error,
            path: format!("{path}.url"),
            message: "backend URL must not be empty".into(),
        });
    } else {
        // Check URL scheme.
        let url_lower = backend.url.to_lowercase();
        let valid_schemes = url_lower.starts_with("ws://")
            || url_lower.starts_with("wss://")
            || url_lower.starts_with("http://")
            || url_lower.starts_with("https://");
        if !valid_schemes {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                path: format!("{path}.url"),
                message: format!(
                    "invalid URL scheme in \"{}\". Expected ws://, wss://, http://, or https://",
                    backend.url
                ),
            });
        }
    }

    if backend.reconnect_interval == 0 {
        issues.push(ValidationIssue {
            severity: Severity::Warning,
            path: format!("{path}.reconnect_interval"),
            message: "reconnect_interval is 0 — this will cause tight reconnect loops".into(),
        });
    }
}

fn validate_protocols(config: &AppConfig, issues: &mut Vec<ValidationIssue>) {
    let has_any_protocol = config
        .protocols
        .onebot_v11
        .as_ref()
        .is_some_and(|c| c.enabled)
        || config
            .protocols
            .onebot_v12
            .as_ref()
            .is_some_and(|c| c.enabled)
        || config.protocols.milky.as_ref().is_some_and(|c| c.enabled)
        || config.protocols.satori.as_ref().is_some_and(|c| c.enabled);

    if !has_any_protocol {
        issues.push(ValidationIssue {
            severity: Severity::Warning,
            path: "protocols".into(),
            message: "no protocol servers enabled — upstream bot frameworks cannot connect".into(),
        });
    }

    if let Some(ref ob) = config.protocols.onebot_v11 {
        if ob.enabled {
            validate_onebot_v11(ob, issues);
        }
    }
}

fn validate_onebot_v11(config: &OneBotV11Config, issues: &mut Vec<ValidationIssue>) {
    if !config.http && !config.ws && config.ws_reverse.is_empty() && config.http_post.is_empty() {
        issues.push(ValidationIssue {
            severity: Severity::Warning,
            path: "protocols.onebot_v11".into(),
            message:
                "OneBot v11 is enabled but no transport (http/ws/ws_reverse/http_post) is active"
                    .into(),
        });
    }

    for (i, target) in config.ws_reverse.iter().enumerate() {
        if target.url.is_empty() {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                path: format!("protocols.onebot_v11.ws_reverse[{i}].url"),
                message: "reverse WS target URL must not be empty".into(),
            });
        }
    }

    for (i, target) in config.http_post.iter().enumerate() {
        if target.url.is_empty() {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                path: format!("protocols.onebot_v11.http_post[{i}].url"),
                message: "HTTP POST target URL must not be empty".into(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;

    fn minimal_valid_config() -> AppConfig {
        serde_yaml::from_str(
            r#"
server:
  port: 8080
accounts:
  - name: main
    backend:
      type: lagrange
      url: "ws://127.0.0.1:8081/onebot/v11/ws"
protocols:
  onebot_v11:
    enabled: true
"#,
        )
        .unwrap()
    }

    #[test]
    fn valid_config_has_no_errors() {
        let config = minimal_valid_config();
        let issues = validate(&config);
        assert!(!has_errors(&issues), "unexpected errors: {issues:?}");
    }

    #[test]
    fn empty_accounts_warns() {
        let mut config = minimal_valid_config();
        config.accounts.clear();
        let issues = validate(&config);
        assert!(
            issues
                .iter()
                .any(|i| i.path == "accounts" && i.severity == Severity::Warning)
        );
    }

    #[test]
    fn duplicate_account_names() {
        let mut config = minimal_valid_config();
        let dup = config.accounts[0].clone();
        config.accounts.push(dup);
        let issues = validate(&config);
        assert!(has_errors(&issues));
        assert!(issues.iter().any(|i| i.message.contains("duplicate")));
    }

    #[test]
    fn empty_backend_url_is_error() {
        let mut config = minimal_valid_config();
        config.accounts[0].backend.url = String::new();
        let issues = validate(&config);
        assert!(has_errors(&issues));
        assert!(issues.iter().any(|i| i.path.contains("url")));
    }

    #[test]
    fn invalid_url_scheme() {
        let mut config = minimal_valid_config();
        config.accounts[0].backend.url = "ftp://localhost".into();
        let issues = validate(&config);
        assert!(has_errors(&issues));
        assert!(issues.iter().any(|i| i.message.contains("scheme")));
    }

    #[test]
    fn no_protocols_warns() {
        let mut config = minimal_valid_config();
        config.protocols = ProtocolsConfig::default();
        let issues = validate(&config);
        assert!(
            issues
                .iter()
                .any(|i| i.path == "protocols" && i.severity == Severity::Warning)
        );
    }

    #[test]
    fn zero_reconnect_interval_warns() {
        let mut config = minimal_valid_config();
        config.accounts[0].backend.reconnect_interval = 0;
        let issues = validate(&config);
        assert!(
            issues
                .iter()
                .any(|i| i.message.contains("reconnect_interval"))
        );
    }
}
