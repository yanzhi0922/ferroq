//! OneBot v11 event parsing.
//!
//! Converts raw OneBot v11 JSON events into internal [`Event`] types.

use chrono::{DateTime, TimeZone, Utc};
use ferroq_core::error::GatewayError;
use ferroq_core::event::*;
use ferroq_core::message::MessageSegment;
use uuid::Uuid;

/// Parse a raw OneBot v11 JSON object into an internal [`Event`].
pub fn parse_event(raw: serde_json::Value) -> Result<Event, GatewayError> {
    let post_type = raw
        .get("post_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GatewayError::Internal("missing post_type".to_string()))?;

    let self_id = raw.get("self_id").and_then(|v| v.as_i64()).unwrap_or(0);

    let time = parse_time(&raw);

    match post_type {
        "message" | "message_sent" => parse_message_event(&raw, self_id, time),
        "notice" => parse_notice_event(raw, self_id, time),
        "request" => parse_request_event(raw, self_id, time),
        "meta_event" => parse_meta_event(raw, self_id, time),
        other => Err(GatewayError::Internal(format!(
            "unknown post_type: {other}"
        ))),
    }
}

/// Parse a unix timestamp from the raw JSON.
fn parse_time(raw: &serde_json::Value) -> DateTime<Utc> {
    raw.get("time")
        .and_then(|v| v.as_i64())
        .and_then(|ts| Utc.timestamp_opt(ts, 0).single())
        .unwrap_or_else(Utc::now)
}

/// Parse a message event.
fn parse_message_event(
    raw: &serde_json::Value,
    self_id: i64,
    time: DateTime<Utc>,
) -> Result<Event, GatewayError> {
    let message_type_str = raw
        .get("message_type")
        .and_then(|v| v.as_str())
        .unwrap_or("private");

    let message_type = match message_type_str {
        "group" => MessageType::Group,
        _ => MessageType::Private,
    };

    let sub_type = raw
        .get("sub_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let message_id = raw.get("message_id").and_then(|v| v.as_i64()).unwrap_or(0);

    let user_id = raw.get("user_id").and_then(|v| v.as_i64()).unwrap_or(0);

    let group_id = raw.get("group_id").and_then(|v| v.as_i64());

    let raw_message = raw
        .get("raw_message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let font = raw.get("font").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

    // Parse message segments.
    let message = parse_message_segments(raw);

    // Parse sender.
    let sender = parse_sender(raw);

    Ok(Event::Message(Box::new(MessageEvent {
        id: Uuid::new_v4(),
        time,
        self_id,
        message_type,
        sub_type,
        message_id,
        user_id,
        group_id,
        message,
        raw_message,
        sender,
        font,
    })))
}

/// Parse message segments from a OneBot v11 message array.
fn parse_message_segments(raw: &serde_json::Value) -> Vec<MessageSegment> {
    let Some(arr) = raw.get("message").and_then(|v| v.as_array()) else {
        // If the message field is a string (CQ-code), just wrap it as text.
        if let Some(text) = raw.get("message").and_then(|v| v.as_str()) {
            return vec![MessageSegment::Text {
                text: text.to_string(),
            }];
        }
        return Vec::new();
    };

    arr.iter()
        .filter_map(|seg| {
            let seg_type = seg.get("type")?.as_str()?;
            let data = seg.get("data").unwrap_or(&serde_json::Value::Null);
            parse_single_segment(seg_type, data)
        })
        .collect()
}

/// Parse a single message segment.
fn parse_single_segment(seg_type: &str, data: &serde_json::Value) -> Option<MessageSegment> {
    match seg_type {
        "text" => Some(MessageSegment::Text {
            text: data.get("text")?.as_str()?.to_string(),
        }),
        "image" => Some(MessageSegment::Image {
            file: data
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            url: data.get("url").and_then(|v| v.as_str()).map(String::from),
        }),
        "at" => Some(MessageSegment::At {
            qq: data
                .get("qq")
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    _ => "0".to_string(),
                })
                .unwrap_or_default(),
        }),
        "face" => Some(MessageSegment::Face {
            id: data
                .get("id")
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    _ => "0".to_string(),
                })
                .unwrap_or_default(),
        }),
        "reply" => Some(MessageSegment::Reply {
            id: data
                .get("id")
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    _ => "0".to_string(),
                })
                .unwrap_or_default(),
        }),
        "record" => Some(MessageSegment::Record {
            file: data
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            url: data.get("url").and_then(|v| v.as_str()).map(String::from),
        }),
        "video" => Some(MessageSegment::Video {
            file: data
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            url: data.get("url").and_then(|v| v.as_str()).map(String::from),
        }),
        "forward" => Some(MessageSegment::Forward {
            id: data
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }),
        "json" => Some(MessageSegment::Json {
            data: data
                .get("data")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }),
        "xml" => Some(MessageSegment::Xml {
            data: data
                .get("data")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }),
        "poke" => Some(MessageSegment::Poke {
            poke_type: data
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            id: data
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }),
        _ => None, // Unknown segment types are dropped.
    }
}

/// Parse a sender object.
fn parse_sender(raw: &serde_json::Value) -> Sender {
    let sender = raw.get("sender");

    Sender {
        user_id: sender
            .and_then(|v| v.get("user_id"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        nickname: sender
            .and_then(|v| v.get("nickname"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        card: sender
            .and_then(|v| v.get("card"))
            .and_then(|v| v.as_str())
            .map(String::from),
        sex: sender
            .and_then(|v| v.get("sex"))
            .and_then(|v| v.as_str())
            .map(String::from),
        age: sender
            .and_then(|v| v.get("age"))
            .and_then(|v| v.as_i64())
            .map(|v| v as i32),
        area: sender
            .and_then(|v| v.get("area"))
            .and_then(|v| v.as_str())
            .map(String::from),
        level: sender
            .and_then(|v| v.get("level"))
            .and_then(|v| v.as_str())
            .map(String::from),
        role: sender
            .and_then(|v| v.get("role"))
            .and_then(|v| v.as_str())
            .map(String::from),
        title: sender
            .and_then(|v| v.get("title"))
            .and_then(|v| v.as_str())
            .map(String::from),
    }
}

/// Parse a notice event.
fn parse_notice_event(
    raw: serde_json::Value,
    self_id: i64,
    time: DateTime<Utc>,
) -> Result<Event, GatewayError> {
    let notice_type = raw
        .get("notice_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let sub_type = raw
        .get("sub_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(Event::Notice(NoticeEvent {
        id: Uuid::new_v4(),
        time,
        self_id,
        notice_type,
        sub_type,
        extra: raw,
    }))
}

/// Parse a request event.
fn parse_request_event(
    raw: serde_json::Value,
    self_id: i64,
    time: DateTime<Utc>,
) -> Result<Event, GatewayError> {
    let request_type = raw
        .get("request_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let sub_type = raw
        .get("sub_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(Event::Request(RequestEvent {
        id: Uuid::new_v4(),
        time,
        self_id,
        request_type,
        sub_type,
        extra: raw,
    }))
}

/// Parse a meta event.
fn parse_meta_event(
    raw: serde_json::Value,
    self_id: i64,
    time: DateTime<Utc>,
) -> Result<Event, GatewayError> {
    let meta_event_type = raw
        .get("meta_event_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let sub_type = raw
        .get("sub_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(Event::Meta(MetaEvent {
        id: Uuid::new_v4(),
        time,
        self_id,
        meta_event_type,
        sub_type,
        extra: raw,
    }))
}

/// Convert an internal [`Event`] back to a OneBot v11 JSON value.
///
/// Used by protocol servers when forwarding events to upstream clients.
pub fn event_to_json(event: &Event) -> serde_json::Value {
    match event {
        Event::Message(msg) => {
            let mut obj = serde_json::json!({
                "post_type": "message",
                "message_type": msg.message_type,
                "sub_type": msg.sub_type,
                "time": msg.time.timestamp(),
                "self_id": msg.self_id,
                "message_id": msg.message_id,
                "user_id": msg.user_id,
                "message": msg.message,
                "raw_message": msg.raw_message,
                "font": msg.font,
                "sender": msg.sender,
            });
            if let Some(group_id) = msg.group_id {
                obj["group_id"] = serde_json::json!(group_id);
            }
            obj
        }
        Event::Notice(evt) => {
            let mut obj = evt.extra.clone();
            if let Some(map) = obj.as_object_mut() {
                map.insert("post_type".to_string(), serde_json::json!("notice"));
                map.insert("time".to_string(), serde_json::json!(evt.time.timestamp()));
                map.insert("self_id".to_string(), serde_json::json!(evt.self_id));
            }
            obj
        }
        Event::Request(evt) => {
            let mut obj = evt.extra.clone();
            if let Some(map) = obj.as_object_mut() {
                map.insert("post_type".to_string(), serde_json::json!("request"));
                map.insert("time".to_string(), serde_json::json!(evt.time.timestamp()));
                map.insert("self_id".to_string(), serde_json::json!(evt.self_id));
            }
            obj
        }
        Event::Meta(evt) => {
            let mut obj = evt.extra.clone();
            if let Some(map) = obj.as_object_mut() {
                map.insert("post_type".to_string(), serde_json::json!("meta_event"));
                map.insert("time".to_string(), serde_json::json!(evt.time.timestamp()));
                map.insert("self_id".to_string(), serde_json::json!(evt.self_id));
            }
            obj
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_group_message() {
        let raw = serde_json::json!({
            "post_type": "message",
            "message_type": "group",
            "sub_type": "normal",
            "time": 1700000000,
            "self_id": 123456,
            "message_id": 99,
            "user_id": 789,
            "group_id": 111,
            "raw_message": "hello",
            "font": 0,
            "message": [
                {"type": "text", "data": {"text": "hello"}}
            ],
            "sender": {
                "user_id": 789,
                "nickname": "test",
                "card": "test card",
                "role": "member"
            }
        });

        let event = parse_event(raw).expect("should parse");
        assert_eq!(event.self_id(), 123456);
        if let Event::Message(msg) = event {
            assert_eq!(msg.message_type, MessageType::Group);
            assert_eq!(msg.group_id, Some(111));
            assert_eq!(msg.message.len(), 1);
            assert_eq!(msg.sender.nickname, "test");
        } else {
            panic!("expected Message event");
        }
    }

    #[test]
    fn parse_meta_heartbeat() {
        let raw = serde_json::json!({
            "post_type": "meta_event",
            "meta_event_type": "heartbeat",
            "time": 1700000000,
            "self_id": 123456,
            "status": {"online": true},
            "interval": 5000
        });

        let event = parse_event(raw).expect("should parse");
        if let Event::Meta(meta) = event {
            assert_eq!(meta.meta_event_type, "heartbeat");
        } else {
            panic!("expected Meta event");
        }
    }

    #[test]
    fn parse_notice_event() {
        let raw = serde_json::json!({
            "post_type": "notice",
            "notice_type": "group_increase",
            "sub_type": "approve",
            "time": 1700000000,
            "self_id": 123456,
            "group_id": 111,
            "user_id": 789,
            "operator_id": 456
        });

        let event = parse_event(raw).expect("should parse");
        assert!(matches!(event, Event::Notice(_)));
    }

    #[test]
    fn event_roundtrip() {
        let raw = serde_json::json!({
            "post_type": "message",
            "message_type": "private",
            "sub_type": "friend",
            "time": 1700000000,
            "self_id": 100,
            "message_id": 1,
            "user_id": 200,
            "raw_message": "hi",
            "font": 0,
            "message": [{"type": "text", "data": {"text": "hi"}}],
            "sender": {"user_id": 200, "nickname": "u"}
        });

        let event = parse_event(raw).expect("parse");
        let json = event_to_json(&event);
        assert_eq!(json["post_type"], "message");
        assert_eq!(json["self_id"], 100);
    }

    #[test]
    fn parse_multiple_segments() {
        let raw = serde_json::json!({
            "post_type": "message",
            "message_type": "group",
            "sub_type": "normal",
            "time": 1700000000,
            "self_id": 100,
            "message_id": 1,
            "user_id": 200,
            "group_id": 300,
            "raw_message": "[CQ:at,qq=123] hello",
            "font": 0,
            "message": [
                {"type": "at", "data": {"qq": "123"}},
                {"type": "text", "data": {"text": " hello"}},
                {"type": "image", "data": {"file": "abc.jpg", "url": "https://example.com/abc.jpg"}}
            ],
            "sender": {"user_id": 200, "nickname": "test"}
        });

        let event = parse_event(raw).unwrap();
        if let Event::Message(msg) = event {
            assert_eq!(msg.message.len(), 3);
            assert!(matches!(&msg.message[0], MessageSegment::At { qq } if qq == "123"));
            assert!(matches!(&msg.message[1], MessageSegment::Text { text } if text == " hello"));
            assert!(
                matches!(&msg.message[2], MessageSegment::Image { file, url } if file == "abc.jpg" && url.is_some())
            );
        } else {
            panic!("expected Message event");
        }
    }
}
