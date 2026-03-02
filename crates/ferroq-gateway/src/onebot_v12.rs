//! OneBot v12 event conversion.
//!
//! Converts between internal [`Event`] types and OneBot v12 JSON format.
//!
//! Key differences from v11:
//! - `type` / `detail_type` instead of `post_type` / `message_type`
//! - `self` object (`{ platform, user_id }`) instead of bare `self_id`
//! - All IDs are strings (not integers)
//! - Action names differ: `send_message` instead of `send_msg`

use ferroq_core::api::{ApiRequest, ApiResponse};
use ferroq_core::error::GatewayError;
use ferroq_core::event::*;
use ferroq_core::message::MessageSegment;

// ---------------------------------------------------------------------------
// Event → v12 JSON (outbound to upstream clients)
// ---------------------------------------------------------------------------

/// Convert an internal Event into a OneBot v12 JSON object.
pub fn event_to_json(event: &Event) -> serde_json::Value {
    match event {
        Event::Message(msg) => message_event_to_v12(msg),
        Event::Notice(evt) => notice_event_to_v12(evt),
        Event::Request(evt) => request_event_to_v12(evt),
        Event::Meta(evt) => meta_event_to_v12(evt),
    }
}

fn self_object(self_id: i64) -> serde_json::Value {
    serde_json::json!({
        "platform": "qq",
        "user_id": self_id.to_string()
    })
}

fn message_event_to_v12(msg: &MessageEvent) -> serde_json::Value {
    let detail_type = match msg.message_type {
        MessageType::Private => "private",
        MessageType::Group => "group",
    };

    let mut obj = serde_json::json!({
        "id": msg.id.to_string(),
        "time": msg.time.timestamp() as f64,
        "type": "message",
        "detail_type": detail_type,
        "sub_type": msg.sub_type,
        "self": self_object(msg.self_id),
        "message_id": msg.message_id.to_string(),
        "message": segments_to_v12(&msg.message),
        "alt_message": msg.raw_message,
        "user_id": msg.user_id.to_string(),
    });

    if let Some(group_id) = msg.group_id {
        obj["group_id"] = serde_json::json!(group_id.to_string());
    }

    obj
}

fn notice_event_to_v12(evt: &NoticeEvent) -> serde_json::Value {
    let detail_type = &evt.notice_type;

    serde_json::json!({
        "id": evt.id.to_string(),
        "time": evt.time.timestamp() as f64,
        "type": "notice",
        "detail_type": detail_type,
        "sub_type": evt.sub_type,
        "self": self_object(evt.self_id),
    })
}

fn request_event_to_v12(evt: &RequestEvent) -> serde_json::Value {
    let detail_type = &evt.request_type;

    serde_json::json!({
        "id": evt.id.to_string(),
        "time": evt.time.timestamp() as f64,
        "type": "request",
        "detail_type": detail_type,
        "sub_type": evt.sub_type,
        "self": self_object(evt.self_id),
    })
}

fn meta_event_to_v12(evt: &MetaEvent) -> serde_json::Value {
    // Map v11 meta events to v12 meta type.
    let detail_type = match evt.meta_event_type.as_str() {
        "heartbeat" => "heartbeat",
        "lifecycle" => "connect",
        _ => &evt.meta_event_type,
    };

    serde_json::json!({
        "id": evt.id.to_string(),
        "time": evt.time.timestamp() as f64,
        "type": "meta",
        "detail_type": detail_type,
        "sub_type": evt.sub_type,
        "self": self_object(evt.self_id),
    })
}

/// Convert message segments to v12 format.
///
/// v12 segment format is very similar to v11 but uses string IDs everywhere.
fn segments_to_v12(segments: &[MessageSegment]) -> serde_json::Value {
    let arr: Vec<serde_json::Value> = segments
        .iter()
        .map(|seg| match seg {
            MessageSegment::Text { text } => serde_json::json!({
                "type": "text",
                "data": { "text": text }
            }),
            MessageSegment::Image { file, url } => {
                let mut data = serde_json::json!({ "file_id": file });
                if let Some(u) = url {
                    data["url"] = serde_json::json!(u);
                }
                serde_json::json!({ "type": "image", "data": data })
            }
            MessageSegment::At { qq } => serde_json::json!({
                "type": "mention",
                "data": { "user_id": qq }
            }),
            MessageSegment::Face { id } => serde_json::json!({
                "type": "qq.face",
                "data": { "id": id }
            }),
            MessageSegment::Reply { id } => serde_json::json!({
                "type": "reply",
                "data": { "message_id": id }
            }),
            MessageSegment::Record { file, url } => {
                let mut data = serde_json::json!({ "file_id": file });
                if let Some(u) = url {
                    data["url"] = serde_json::json!(u);
                }
                serde_json::json!({ "type": "voice", "data": data })
            }
            MessageSegment::Video { file, url } => {
                let mut data = serde_json::json!({ "file_id": file });
                if let Some(u) = url {
                    data["url"] = serde_json::json!(u);
                }
                serde_json::json!({ "type": "video", "data": data })
            }
            MessageSegment::Forward { id } => serde_json::json!({
                "type": "qq.forward",
                "data": { "id": id }
            }),
            MessageSegment::Json { data } => serde_json::json!({
                "type": "qq.json",
                "data": { "data": data }
            }),
            MessageSegment::Xml { data } => serde_json::json!({
                "type": "qq.xml",
                "data": { "data": data }
            }),
            MessageSegment::Poke { poke_type, id } => serde_json::json!({
                "type": "qq.poke",
                "data": { "type": poke_type, "id": id }
            }),
            MessageSegment::Unknown => serde_json::json!({
                "type": "unknown",
                "data": {}
            }),
        })
        .collect();

    serde_json::Value::Array(arr)
}

// ---------------------------------------------------------------------------
// v12 JSON → internal Event (inbound from v12 clients — less common but useful)
// ---------------------------------------------------------------------------

/// Parse a v12 API request JSON into an internal [`ApiRequest`].
///
/// v12 uses the same basic structure but with different action names and
/// string-typed IDs. We translate to the v11-style action names that the
/// backend router expects.
pub fn parse_v12_action(raw: serde_json::Value) -> Result<ApiRequest, GatewayError> {
    let action = raw
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GatewayError::Internal("missing action field".to_string()))?
        .to_string();

    let params = raw
        .get("params")
        .cloned()
        .unwrap_or(serde_json::Value::Object(Default::default()));

    let echo = raw.get("echo").cloned();

    // Extract self_id from the v12 "self" object.
    let self_id = raw
        .get("self")
        .and_then(|s| s.get("user_id"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok());

    // Translate v12 action names to v11 equivalents for the backend router.
    let (translated_action, translated_params) = translate_v12_action(&action, params);

    Ok(ApiRequest {
        action: translated_action,
        params: translated_params,
        echo,
        self_id,
    })
}

/// Translate a v12 action name and params into v11 equivalents.
fn translate_v12_action(
    action: &str,
    params: serde_json::Value,
) -> (String, serde_json::Value) {
    match action {
        "send_message" => {
            // v12 `send_message` → v11 `send_msg` / `send_group_msg` / `send_private_msg`
            let detail_type = params
                .get("detail_type")
                .and_then(|v| v.as_str())
                .unwrap_or("private");

            let v11_action = match detail_type {
                "group" => "send_group_msg",
                "private" => "send_private_msg",
                _ => "send_msg",
            };

            // Convert string IDs to integer IDs for v11 backends.
            let mut v11_params = serde_json::Map::new();
            if let Some(group_id) = params.get("group_id").and_then(|v| v.as_str()) {
                if let Ok(id) = group_id.parse::<i64>() {
                    v11_params.insert("group_id".to_string(), serde_json::json!(id));
                }
            }
            if let Some(user_id) = params.get("user_id").and_then(|v| v.as_str()) {
                if let Ok(id) = user_id.parse::<i64>() {
                    v11_params.insert("user_id".to_string(), serde_json::json!(id));
                }
            }
            v11_params.insert(
                "message_type".to_string(),
                serde_json::json!(detail_type),
            );
            // Pass through message segments as-is (v12 format is compatible).
            if let Some(msg) = params.get("message") {
                v11_params.insert("message".to_string(), msg.clone());
            }

            (v11_action.to_string(), serde_json::Value::Object(v11_params))
        }
        "get_self_info" => ("get_login_info".to_string(), params),
        "get_message" => {
            // Convert string message_id to integer.
            let mut v11_params = params.clone();
            if let Some(id_str) = params.get("message_id").and_then(|v| v.as_str()) {
                if let Ok(id) = id_str.parse::<i64>() {
                    v11_params["message_id"] = serde_json::json!(id);
                }
            }
            ("get_msg".to_string(), v11_params)
        }
        "get_user_info" => ("get_stranger_info".to_string(), params),
        "get_group_info" => ("get_group_info".to_string(), params),
        "get_group_member_info" => ("get_group_member_info".to_string(), params),
        "get_group_member_list" => ("get_group_member_list".to_string(), params),
        "set_group_name" => ("set_group_name".to_string(), params),
        "leave_group" => ("set_group_leave".to_string(), params),
        "kick_group_member" => ("set_group_kick".to_string(), params),
        "ban_group_member" => ("set_group_ban".to_string(), params),
        "get_friend_list" => ("get_friend_list".to_string(), params),
        "get_group_list" => ("get_group_list".to_string(), params),
        // Pass through unknown actions as-is.
        _ => (action.to_string(), params),
    }
}

/// Translate a v11 API response to v12 format.
///
/// The response structure is mostly compatible; we only need to convert
/// specific data shapes (e.g., integer IDs → strings) for certain actions.
pub fn translate_v11_response(action: &str, resp: ApiResponse) -> serde_json::Value {
    let data = match action {
        "get_self_info" | "get_login_info" => {
            // v11 returns { user_id: int, nickname: str }
            // v12 expects { user_id: str, user_name: str, user_displayname: str }
            if let Some(user_id) = resp.data.get("user_id") {
                let mut v12_data = serde_json::Map::new();
                let uid_str = match user_id {
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                v12_data.insert(
                    "user_id".to_string(),
                    serde_json::json!(uid_str),
                );
                v12_data.insert(
                    "user_name".to_string(),
                    resp.data
                        .get("nickname")
                        .cloned()
                        .unwrap_or(serde_json::json!("")),
                );
                v12_data.insert(
                    "user_displayname".to_string(),
                    resp.data
                        .get("nickname")
                        .cloned()
                        .unwrap_or(serde_json::json!("")),
                );
                serde_json::Value::Object(v12_data)
            } else {
                resp.data
            }
        }
        _ => resp.data,
    };

    serde_json::json!({
        "status": resp.status,
        "retcode": resp.retcode,
        "data": data,
        "message": resp.message,
        "echo": resp.echo,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    #[test]
    fn event_to_v12_message() {
        let event = Event::Message(Box::new(MessageEvent {
            id: Uuid::nil(),
            time: Utc.timestamp_opt(1700000000, 0).unwrap(),
            self_id: 123456,
            message_type: MessageType::Group,
            sub_type: "normal".to_string(),
            message_id: 99,
            user_id: 789,
            group_id: Some(111),
            message: vec![MessageSegment::Text {
                text: "hello".to_string(),
            }],
            raw_message: "hello".to_string(),
            sender: Sender {
                user_id: 789,
                nickname: "test".to_string(),
                card: None,
                sex: None,
                age: None,
                area: None,
                level: None,
                role: None,
                title: None,
            },
            font: 0,
        }));

        let json = event_to_json(&event);
        assert_eq!(json["type"], "message");
        assert_eq!(json["detail_type"], "group");
        assert_eq!(json["self"]["platform"], "qq");
        assert_eq!(json["self"]["user_id"], "123456");
        assert_eq!(json["user_id"], "789");
        assert_eq!(json["group_id"], "111");
        assert_eq!(json["message_id"], "99");

        // Message segments
        let msg = json["message"].as_array().unwrap();
        assert_eq!(msg.len(), 1);
        assert_eq!(msg[0]["type"], "text");
        assert_eq!(msg[0]["data"]["text"], "hello");
    }

    #[test]
    fn v12_at_segment_uses_mention() {
        let segs = vec![MessageSegment::At {
            qq: "123".to_string(),
        }];
        let json = segments_to_v12(&segs);
        let arr = json.as_array().unwrap();
        assert_eq!(arr[0]["type"], "mention");
        assert_eq!(arr[0]["data"]["user_id"], "123");
    }

    #[test]
    fn translate_send_message_group() {
        let params = serde_json::json!({
            "detail_type": "group",
            "group_id": "12345",
            "message": [{"type": "text", "data": {"text": "hi"}}]
        });

        let (action, translated) = translate_v12_action("send_message", params);
        assert_eq!(action, "send_group_msg");
        assert_eq!(translated["group_id"], 12345);
        assert_eq!(translated["message_type"], "group");
    }

    #[test]
    fn translate_get_self_info() {
        let (action, _) =
            translate_v12_action("get_self_info", serde_json::Value::Object(Default::default()));
        assert_eq!(action, "get_login_info");
    }

    #[test]
    fn parse_v12_action_request() {
        let raw = serde_json::json!({
            "action": "send_message",
            "params": {
                "detail_type": "private",
                "user_id": "789",
                "message": [{"type": "text", "data": {"text": "hello"}}]
            },
            "echo": "abc",
            "self": {
                "platform": "qq",
                "user_id": "123456"
            }
        });

        let req = parse_v12_action(raw).unwrap();
        assert_eq!(req.action, "send_private_msg");
        assert_eq!(req.self_id, Some(123456));
        assert_eq!(req.echo, Some(serde_json::json!("abc")));
    }
}
