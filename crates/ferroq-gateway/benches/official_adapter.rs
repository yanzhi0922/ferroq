//! Official adapter benchmark (HTTP action path).

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use ferroq_core::adapter::BackendAdapter;
use ferroq_core::api::{ApiRequest, ApiResponse};
use ferroq_gateway::adapter::OfficialAdapter;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

async fn start_mock_official_server() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    async fn get_login_info() -> axum::Json<ApiResponse> {
        axum::Json(ApiResponse::ok(serde_json::json!({
            "user_id": 20260001_i64,
            "nickname": "OfficialBenchBot",
        })))
    }

    async fn get_status() -> axum::Json<ApiResponse> {
        axum::Json(ApiResponse::ok(serde_json::json!({
            "online": true,
            "good": true,
        })))
    }

    let app = axum::Router::new()
        .route("/api/get_login_info", axum::routing::post(get_login_info))
        .route("/api/get_status", axum::routing::post(get_status));

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock official server");
    let addr = listener.local_addr().expect("local addr");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    (addr, handle)
}

fn bench_official_adapter_http(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("create runtime");

    let (addr, handle) = rt.block_on(start_mock_official_server());

    let adapter = rt.block_on(async {
        let adapter = OfficialAdapter::new("official-bench", format!("http://{addr}"), "", 2, 2)
            .expect("create adapter");
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        adapter.connect(event_tx).await.expect("connect adapter");
        adapter
    });

    // Warm up one call to lock in route mode discovery before timing.
    rt.block_on(async {
        let _ = adapter
            .call_api(ApiRequest {
                action: "get_status".to_string(),
                params: serde_json::json!({}),
                echo: None,
                self_id: None,
            })
            .await
            .expect("warm up call");
    });

    c.bench_function("official_http/call_api_get_login_info", |b| {
        b.to_async(&rt).iter(|| async {
            let response = adapter
                .call_api(ApiRequest {
                    action: "get_login_info".to_string(),
                    params: serde_json::json!({}),
                    echo: None,
                    self_id: None,
                })
                .await
                .expect("call_api should succeed");
            black_box(response);
        });
    });

    c.bench_function("official_http/call_api_get_status", |b| {
        b.to_async(&rt).iter(|| async {
            let response = adapter
                .call_api(ApiRequest {
                    action: "get_status".to_string(),
                    params: serde_json::json!({}),
                    echo: None,
                    self_id: None,
                })
                .await
                .expect("call_api should succeed");
            black_box(response);
        });
    });

    rt.block_on(async {
        let _ = adapter.disconnect().await;
    });
    handle.abort();
}

criterion_group!(benches, bench_official_adapter_http);
criterion_main!(benches);
