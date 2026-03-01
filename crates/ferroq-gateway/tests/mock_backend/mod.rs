//! Mock OneBot v11 backend server for integration tests.
//!
//! Simulates a Lagrange.OneBot forward WebSocket endpoint. It:
//! - Accepts WS connections at `/onebot/v11/ws`
//! - Sends periodic heartbeat meta events
//! - Responds to API calls (e.g. `get_login_info`, `get_status`, `send_group_msg`)

use std::net::SocketAddr;

use axum::extract::ws::{Message as AxumWsMessage, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::any;
use futures::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

const MOCK_SELF_ID: i64 = 1234567890;

/// Start the mock backend server. Returns the address and a JoinHandle.
pub async fn start() -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock backend");
    let addr = listener.local_addr().expect("local addr");

    let app = axum::Router::new().route("/onebot/v11/ws", any(ws_handler));

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    // Give the server a moment to start.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    (addr, handle)
}

async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_ws)
}

async fn handle_ws(socket: WebSocket) {
    let (mut tx, mut rx) = socket.split();

    // Spawn heartbeat sender — sends a heartbeat meta event every 200ms.
    let heartbeat_tx = {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<String>();

        // Writer task: writes messages from the channel to WS.
        let _writer = tokio::spawn(async move {
            while let Some(text) = receiver.recv().await {
                if tx.send(AxumWsMessage::Text(text.into())).await.is_err() {
                    break;
                }
            }
        });

        // Heartbeat producer: sends periodic heartbeat events.
        let heartbeat_sender = sender.clone();
        let _heartbeat = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(200));
            loop {
                interval.tick().await;
                let event = serde_json::json!({
                    "time": chrono::Utc::now().timestamp(),
                    "self_id": MOCK_SELF_ID,
                    "post_type": "meta_event",
                    "meta_event_type": "heartbeat",
                    "status": {
                        "online": true,
                        "good": true,
                    },
                    "interval": 200,
                });
                let text = serde_json::to_string(&event).unwrap();
                if heartbeat_sender.send(text).is_err() {
                    break;
                }
            }
        });

        sender
    };

    // Read incoming messages (API requests) and respond.
    while let Some(Ok(msg)) = rx.next().await {
        match msg {
            AxumWsMessage::Text(text) => {
                let json: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let action = json["action"].as_str().unwrap_or("");
                let echo = json.get("echo").cloned();

                let response = make_api_response(action, &json, echo);
                let resp_text = serde_json::to_string(&response).unwrap();
                if heartbeat_tx.send(resp_text).is_err() {
                    break;
                }
            }
            AxumWsMessage::Close(_) => break,
            _ => {}
        }
    }
}

fn make_api_response(
    action: &str,
    _request: &serde_json::Value,
    echo: Option<serde_json::Value>,
) -> serde_json::Value {
    match action {
        "get_login_info" => serde_json::json!({
            "status": "ok",
            "retcode": 0,
            "data": {
                "user_id": MOCK_SELF_ID,
                "nickname": "TestBot",
            },
            "echo": echo,
        }),

        "get_status" => serde_json::json!({
            "status": "ok",
            "retcode": 0,
            "data": {
                "online": true,
                "good": true,
            },
            "echo": echo,
        }),

        "send_group_msg" | "send_private_msg" | "send_msg" => serde_json::json!({
            "status": "ok",
            "retcode": 0,
            "data": {
                "message_id": 99999,
            },
            "echo": echo,
        }),

        "get_group_list" => serde_json::json!({
            "status": "ok",
            "retcode": 0,
            "data": [
                {
                    "group_id": 123456,
                    "group_name": "Test Group",
                    "member_count": 10,
                    "max_member_count": 200,
                }
            ],
            "echo": echo,
        }),

        "get_friend_list" => serde_json::json!({
            "status": "ok",
            "retcode": 0,
            "data": [
                {
                    "user_id": 11111,
                    "nickname": "Friend1",
                    "remark": "",
                }
            ],
            "echo": echo,
        }),

        // Default: echo back as unknown action.
        _ => serde_json::json!({
            "status": "failed",
            "retcode": 1404,
            "data": null,
            "message": format!("unknown action: {action}"),
            "echo": echo,
        }),
    }
}
