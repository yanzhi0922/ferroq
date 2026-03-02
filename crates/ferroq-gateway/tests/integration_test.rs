//! Integration tests for the ferroq gateway.
//!
//! These tests spin up a mock OneBot v11 backend (WebSocket server),
//! connect a `LagrangeAdapter` to it, and verify the full pipeline:
//!
//!   mock backend → adapter → event bus → protocol server → upstream client.

use std::sync::Arc;
use std::time::Duration;

use ferroq_core::adapter::BackendAdapter;
use ferroq_core::api::{ApiRequest, ApiResponse};
use ferroq_core::config::*;
use ferroq_core::event::Event;
use ferroq_gateway::adapter::LagrangeAdapter;
use ferroq_gateway::bus::EventBus;
use ferroq_gateway::router::ApiRouter;
use ferroq_gateway::server::OneBotV11Server;
use ferroq_gateway::shared_config::SharedConfig;
use ferroq_gateway::stats::RuntimeStats;
use futures::StreamExt;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::time::timeout;

mod mock_backend;

// ---------------------------------------------------------------------------
// Test: Adapter connects and receives events from mock backend
// ---------------------------------------------------------------------------

#[tokio::test]
async fn adapter_receives_events_from_mock_backend() {
    let (addr, _handle) = mock_backend::start().await;

    let adapter = LagrangeAdapter::new(
        "test",
        format!("ws://{}/onebot/v11/ws", addr),
        "",
        5,
        120,
        30,
        15,
        30,
    );

    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<Event>();
    adapter.connect(event_tx).await.expect("connect should succeed");

    // The mock backend sends a heartbeat event every 200ms.
    let event = timeout(Duration::from_secs(3), event_rx.recv())
        .await
        .expect("should receive event within 3s")
        .expect("channel should not be closed");

    match &event {
        Event::Meta(meta) => {
            assert_eq!(meta.meta_event_type, "heartbeat");
            assert_eq!(meta.self_id, 1234567890);
        }
        other => panic!("expected Meta event, got: {other:?}"),
    };

    adapter.disconnect().await.expect("disconnect ok");
}

// ---------------------------------------------------------------------------
// Test: Adapter call_api round-trip through mock backend
// ---------------------------------------------------------------------------

#[tokio::test]
async fn adapter_call_api_roundtrip() {
    let (addr, _handle) = mock_backend::start().await;

    let adapter = LagrangeAdapter::new(
        "test-api",
        format!("ws://{}/onebot/v11/ws", addr),
        "",
        5,
        120,
        30,
        15,
        30,
    );

    let (event_tx, _event_rx) = mpsc::unbounded_channel::<Event>();
    adapter.connect(event_tx).await.expect("connect should succeed");

    // Give connection a moment to stabilize.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let response = adapter
        .call_api(ApiRequest {
            action: "get_login_info".to_string(),
            params: serde_json::Value::Object(Default::default()),
            echo: None,
            self_id: None,
        })
        .await
        .expect("call_api should succeed");

    assert_eq!(response.status, "ok");
    assert_eq!(response.retcode, 0);
    assert_eq!(response.data["user_id"], 1234567890);
    assert_eq!(response.data["nickname"], "TestBot");

    adapter.disconnect().await.expect("disconnect ok");
}

// ---------------------------------------------------------------------------
// Test: Full pipeline — mock backend → adapter → bus → OneBot v11 HTTP API
// ---------------------------------------------------------------------------

#[tokio::test]
async fn full_pipeline_http_api() {
    let (backend_addr, _backend_handle) = mock_backend::start().await;

    // Create components.
    let bus = Arc::new(EventBus::new());
    let router = Arc::new(ApiRouter::new());

    // Create and connect the adapter.
    let adapter = Arc::new(LagrangeAdapter::new(
        "pipeline-test",
        format!("ws://{}/onebot/v11/ws", backend_addr),
        "",
        5,
        120,
        30,
        15,
        30,
    ));

    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<Event>();
    adapter.connect(event_tx).await.expect("connect");
    router.register(Arc::clone(&adapter) as Arc<dyn ferroq_core::adapter::BackendAdapter>);

    // Spawn event forwarding (adapter → bus).
    let bus_clone = Arc::clone(&bus);
    let _fwd = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            bus_clone.publish(event);
        }
    });

    // Build the OneBot v11 protocol server.
    let ob_config = OneBotV11Config {
        enabled: true,
        http: true,
        ws: true,
        ws_reverse: vec![],
        http_post: vec![],
    };
    let server = OneBotV11Server::new(ob_config, Arc::new(SharedConfig::new(String::new())));
    let stats = Arc::new(RuntimeStats::new());
    let app = server.build_router(Arc::clone(&router), bus.raw_sender(), stats);

    // Start the HTTP server.
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let server_addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    // Give everything a moment.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Call the OneBot v11 HTTP API.
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{}/api/get_login_info", server_addr))
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await
        .expect("HTTP request failed");

    assert_eq!(resp.status(), 200);

    let body: ApiResponse = resp.json().await.expect("parse response");
    assert_eq!(body.status, "ok");
    assert_eq!(body.retcode, 0);
    assert_eq!(body.data["user_id"], 1234567890);

    let _ = adapter.disconnect().await;
}

// ---------------------------------------------------------------------------
// Test: Full pipeline — events propagate to bus subscribers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn events_propagate_through_bus() {
    let (backend_addr, _backend_handle) = mock_backend::start().await;

    let bus = Arc::new(EventBus::new());
    let mut bus_rx = bus.subscribe();

    let adapter = LagrangeAdapter::new(
        "bus-test",
        format!("ws://{}/onebot/v11/ws", backend_addr),
        "",
        5,
        120,
        30,
        15,
        30,
    );

    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<Event>();
    adapter.connect(event_tx).await.expect("connect");

    // Forward adapter events to bus.
    let bus_clone = Arc::clone(&bus);
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            bus_clone.publish(event);
        }
    });

    // The subscriber should receive the mock heartbeat.
    let event = timeout(Duration::from_secs(3), bus_rx.recv())
        .await
        .expect("should receive within 3s")
        .expect("bus recv should succeed");

    match event {
        Event::Meta(meta) => {
            assert_eq!(meta.meta_event_type, "heartbeat");
        }
        other => panic!("expected Meta, got: {other:?}"),
    };

    let _ = adapter.disconnect().await;
}

// ---------------------------------------------------------------------------
// Test: Forward WebSocket receives events
// ---------------------------------------------------------------------------

#[tokio::test]
async fn forward_ws_receives_events() {
    let (backend_addr, _backend_handle) = mock_backend::start().await;

    let bus = Arc::new(EventBus::new());
    let router = Arc::new(ApiRouter::new());

    let adapter = Arc::new(LagrangeAdapter::new(
        "ws-test",
        format!("ws://{}/onebot/v11/ws", backend_addr),
        "",
        5,
        120,
        30,
        15,
        30,
    ));

    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<Event>();
    adapter.connect(event_tx).await.expect("connect");
    router.register(Arc::clone(&adapter) as Arc<dyn ferroq_core::adapter::BackendAdapter>);

    let bus_clone = Arc::clone(&bus);
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            bus_clone.publish(event);
        }
    });

    let ob_config = OneBotV11Config {
        enabled: true,
        http: true,
        ws: true,
        ws_reverse: vec![],
        http_post: vec![],
    };
    let server = OneBotV11Server::new(ob_config, Arc::new(SharedConfig::new(String::new())));
    let stats = Arc::new(RuntimeStats::new());
    let app = server.build_router(Arc::clone(&router), bus.raw_sender(), stats);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let server_addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect a WS client to the forward WS endpoint.
    let ws_url = format!("ws://{}/ws", server_addr);
    let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("forward WS connect");

    let (_ws_tx, mut ws_rx) = ws_stream.split();

    // Should receive an event pushed from the backend via the bus.
    let msg = timeout(Duration::from_secs(5), ws_rx.next())
        .await
        .expect("should receive within 5s")
        .expect("stream should have a message")
        .expect("message should be Ok");

    let text = msg.into_text().expect("should be text");
    let json: serde_json::Value = serde_json::from_str(&text).expect("should be JSON");
    assert_eq!(json["post_type"], "meta_event");
    assert_eq!(json["meta_event_type"], "heartbeat");

    let _ = adapter.disconnect().await;
}
