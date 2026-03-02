//! Shared helpers for benchmark suites.

#![allow(dead_code)]

use chrono::Utc;
use ferroq_core::event::{Event, MessageEvent, MessageType, MetaEvent, Sender};
use ferroq_core::message::MessageSegment;
use uuid::Uuid;

/// Create a minimal message event for benchmarking.
pub fn make_message_event(self_id: i64, message_id: i64) -> Event {
    Event::Message(Box::new(MessageEvent {
        id: Uuid::new_v4(),
        time: Utc::now(),
        self_id,
        message_type: MessageType::Group,
        sub_type: "normal".to_string(),
        message_id,
        user_id: 10001,
        group_id: Some(900001),
        message: vec![MessageSegment::Text {
            text: "hello world".to_string(),
        }],
        raw_message: "hello world".to_string(),
        sender: Sender {
            user_id: 10001,
            nickname: "bench-user".to_string(),
            card: None,
            sex: None,
            age: None,
            area: None,
            level: None,
            role: None,
            title: None,
        },
        font: 0,
    }))
}

/// Create a message event with ~1KB payload (text segments).
pub fn make_1kb_message_event(self_id: i64, message_id: i64) -> Event {
    // Each text segment is about 100 bytes of JSON. 10 segments ≈ 1KB.
    let segments: Vec<MessageSegment> = (0..10)
        .map(|i| MessageSegment::Text {
            text: format!(
                "benchmark payload segment {i}: {}",
                "x".repeat(80)
            ),
        })
        .collect();
    let raw = segments
        .iter()
        .map(|s| match s {
            MessageSegment::Text { text } => text.as_str(),
            _ => "",
        })
        .collect::<Vec<_>>()
        .join("");

    Event::Message(Box::new(MessageEvent {
        id: Uuid::new_v4(),
        time: Utc::now(),
        self_id,
        message_type: MessageType::Group,
        sub_type: "normal".to_string(),
        message_id,
        user_id: 10001,
        group_id: Some(900001),
        message: segments,
        raw_message: raw,
        sender: Sender {
            user_id: 10001,
            nickname: "bench-user".to_string(),
            card: None,
            sex: None,
            age: None,
            area: None,
            level: None,
            role: None,
            title: None,
        },
        font: 0,
    }))
}

/// Create a heartbeat meta event.
pub fn make_heartbeat_event(self_id: i64) -> Event {
    Event::Meta(MetaEvent {
        id: Uuid::new_v4(),
        time: Utc::now(),
        self_id,
        meta_event_type: "heartbeat".to_string(),
        sub_type: String::new(),
        extra: serde_json::json!({"status": {"online": true}}),
    })
}

/// Create a raw OneBot v11 message JSON for parsing benchmarks.
pub fn make_raw_onebot_v11_json(self_id: i64, message_id: i64) -> serde_json::Value {
    serde_json::json!({
        "post_type": "message",
        "message_type": "group",
        "sub_type": "normal",
        "self_id": self_id,
        "time": Utc::now().timestamp(),
        "message_id": message_id,
        "user_id": 10001,
        "group_id": 900001,
        "message": [
            {"type": "text", "data": {"text": "hello world"}}
        ],
        "raw_message": "hello world",
        "sender": {
            "user_id": 10001,
            "nickname": "bench-user"
        },
        "font": 0
    })
}

/// Create a raw OneBot v11 message JSON with ~1KB payload.
pub fn make_raw_onebot_v11_1kb_json(self_id: i64, message_id: i64) -> serde_json::Value {
    let segments: Vec<serde_json::Value> = (0..10)
        .map(|i| {
            serde_json::json!({
                "type": "text",
                "data": {"text": format!("benchmark payload segment {i}: {}", "x".repeat(80))}
            })
        })
        .collect();

    serde_json::json!({
        "post_type": "message",
        "message_type": "group",
        "sub_type": "normal",
        "self_id": self_id,
        "time": Utc::now().timestamp(),
        "message_id": message_id,
        "user_id": 10001,
        "group_id": 900001,
        "message": segments,
        "raw_message": "x".repeat(1000),
        "sender": {
            "user_id": 10001,
            "nickname": "bench-user"
        },
        "font": 0
    })
}
