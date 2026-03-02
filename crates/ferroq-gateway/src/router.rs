//! API router — routes API requests to the correct backend adapter.

use std::collections::HashMap;
use std::sync::Arc;

use ferroq_core::adapter::BackendAdapter;
use ferroq_core::api::{ApiRequest, ApiResponse};
use ferroq_core::error::GatewayError;
use parking_lot::RwLock;
use tracing::{debug, warn};

/// Routes API requests to the appropriate backend adapter based on `self_id`.
pub struct ApiRouter {
    /// Map from self_id → adapter index.
    routing_table: RwLock<HashMap<i64, usize>>,
    /// Ordered list of adapters.
    adapters: RwLock<Vec<Arc<dyn BackendAdapter>>>,
    /// Default adapter index (first connected adapter).
    default_index: RwLock<Option<usize>>,
}

impl ApiRouter {
    /// Create an empty router.
    pub fn new() -> Self {
        Self {
            routing_table: RwLock::new(HashMap::new()),
            adapters: RwLock::new(Vec::new()),
            default_index: RwLock::new(None),
        }
    }

    /// Register a backend adapter, optionally associating it with a self_id.
    pub fn register(&self, adapter: Arc<dyn BackendAdapter>) {
        let info = adapter.info();
        let mut adapters = self.adapters.write();
        let index = adapters.len();
        adapters.push(adapter);

        if let Some(self_id) = info.self_id {
            self.routing_table.write().insert(self_id, index);
        }

        // First adapter becomes default.
        let mut default = self.default_index.write();
        if default.is_none() {
            *default = Some(index);
        }

        debug!(name = %info.name, backend = %info.backend_type, "registered adapter #{index}");
    }

    /// Update the routing table when a self_id becomes known.
    pub fn associate_self_id(&self, self_id: i64, adapter_index: usize) {
        self.routing_table.write().insert(self_id, adapter_index);
        debug!(self_id, adapter_index, "associated self_id with adapter");
    }

    /// Route an API request to the appropriate backend.
    pub async fn route(&self, request: ApiRequest) -> Result<ApiResponse, GatewayError> {
        let adapter = {
            let adapters = self.adapters.read();
            if adapters.is_empty() {
                return Err(GatewayError::Internal("no adapters registered".to_string()));
            }

            let index = if let Some(self_id) = request.self_id {
                let table = self.routing_table.read();
                match table.get(&self_id) {
                    Some(&idx) => idx,
                    None => {
                        warn!(self_id, "no adapter for self_id, using default");
                        self.default_index.read().unwrap_or(0)
                    }
                }
            } else {
                self.default_index.read().unwrap_or(0)
            };

            adapters.get(index).cloned().ok_or_else(|| {
                GatewayError::Internal(format!("adapter index {index} out of bounds"))
            })?
        };

        adapter.call_api(request).await
    }
}

impl Default for ApiRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use ferroq_core::adapter::{AdapterInfo, AdapterState, BackendAdapter};
    use ferroq_core::api::{ApiRequest, ApiResponse};
    use ferroq_core::error::GatewayError;
    use ferroq_core::event::Event;
    use tokio::sync::mpsc;

    /// A minimal mock adapter for router tests.
    struct MockAdapter {
        name: String,
        self_id: Option<i64>,
    }

    impl MockAdapter {
        fn new(name: &str, self_id: Option<i64>) -> Arc<Self> {
            Arc::new(Self {
                name: name.to_string(),
                self_id,
            })
        }
    }

    #[async_trait]
    impl BackendAdapter for MockAdapter {
        fn info(&self) -> AdapterInfo {
            AdapterInfo {
                name: self.name.clone(),
                backend_type: "mock".to_string(),
                url: "ws://mock".to_string(),
                state: AdapterState::Connected,
                self_id: self.self_id,
            }
        }
        async fn connect(&self, _tx: mpsc::UnboundedSender<Event>) -> Result<(), GatewayError> {
            Ok(())
        }
        async fn disconnect(&self) -> Result<(), GatewayError> {
            Ok(())
        }
        async fn call_api(&self, _req: ApiRequest) -> Result<ApiResponse, GatewayError> {
            Ok(ApiResponse {
                status: "ok".to_string(),
                retcode: 0,
                data: serde_json::json!({ "adapter": self.name }),
                message: String::new(),
                echo: None,
            })
        }
        async fn health_check(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn associate_self_id_routes_correctly() {
        let router = ApiRouter::new();

        // Register two adapters, neither knows their self_id yet.
        let a1 = MockAdapter::new("bot-a", None);
        let a2 = MockAdapter::new("bot-b", None);
        router.register(a1 as Arc<dyn BackendAdapter>);
        router.register(a2 as Arc<dyn BackendAdapter>);

        // Without self_id, requests go to the default (index 0 = bot-a).
        let resp = router
            .route(ApiRequest {
                action: "test".into(),
                params: serde_json::Value::Null,
                echo: None,
                self_id: None,
            })
            .await
            .unwrap();
        assert_eq!(resp.data["adapter"], "bot-a");

        // Now dynamically associate self_id 99 with adapter index 1 (bot-b).
        router.associate_self_id(99, 1);

        // Request with self_id=99 should now route to bot-b.
        let resp = router
            .route(ApiRequest {
                action: "test".into(),
                params: serde_json::Value::Null,
                echo: None,
                self_id: Some(99),
            })
            .await
            .unwrap();
        assert_eq!(resp.data["adapter"], "bot-b");
    }

    #[tokio::test]
    async fn register_with_known_self_id() {
        let router = ApiRouter::new();

        // Adapter already knows its self_id at registration time.
        let a = MockAdapter::new("known-bot", Some(42));
        router.register(a as Arc<dyn BackendAdapter>);

        let resp = router
            .route(ApiRequest {
                action: "test".into(),
                params: serde_json::Value::Null,
                echo: None,
                self_id: Some(42),
            })
            .await
            .unwrap();
        assert_eq!(resp.data["adapter"], "known-bot");
    }
}
