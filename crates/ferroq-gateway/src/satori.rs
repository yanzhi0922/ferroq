//! Satori protocol conversion layer.
//!
//! Converts between internal [`Event`] / [`ApiRequest`] types and the
//! [Satori protocol](https://satori.chat/) JSON format.
//!
//! ## Key differences from OneBot
//!
//! - HTTP RPC: `POST /v1/{resource}.{method}` instead of action-based
//! - Message content uses an HTML-like element encoding (not segment arrays)
//! - Events are pushed via WebSocket with opcode-based framing
//! - All IDs are strings
//! - Headers: `Satori-Platform`, `Satori-User-ID`, `Authorization: Bearer`

use ferroq_core::api::{ApiRequest, ApiResponse};
use ferroq_core::error::GatewayError;
use ferroq_core::event::*;
use ferroq_core::message::MessageSegment;

// ---------------------------------------------------------------------------
// Satori protocol opcodes
// ---------------------------------------------------------------------------

/// WebSocket signal opcodes per the Satori spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    /// Server → Client: an event.
    Event = 0,
    /// Client → Server: heartbeat ping.
    Ping = 1,
    /// Server → Client: heartbeat pong.
    Pong = 2,
    /// Client → Server: authentication.
    Identify = 3,
    /// Server → Client: auth success.
    Ready = 4,
    /// Server → Client: metadata update.
    Meta = 5,
}

// ---------------------------------------------------------------------------
// Internal Event → Satori JSON (outbound)
// ---------------------------------------------------------------------------

/// Convert an internal Event into a Satori protocol event JSON.
///
/// Returns a full WebSocket signal: `{ "op": 0, "body": { ... } }`.
pub fn event_to_signal(event: &Event, sn: u64) -> serde_json::Value {
    let body = event_to_satori(event, sn);
    serde_json::json!({
        "op": Opcode::Event as u8,
        "body": body,
    })
}

/// Convert an internal Event into a Satori event body.
pub fn event_to_satori(event: &Event, sn: u64) -> serde_json::Value {
    match event {
        Event::Message(msg) => message_event_to_satori(msg, sn),
        Event::Notice(evt) => notice_event_to_satori(evt, sn),
        Event::Request(evt) => request_event_to_satori(evt, sn),
        Event::Meta(evt) => meta_event_to_satori(evt, sn),
    }
}

fn login_object(self_id: i64) -> serde_json::Value {
    serde_json::json!({
        "sn": 0,
        "platform": "qq",
        "user": {
            "id": self_id.to_string(),
            "name": self_id.to_string(),
        },
        "status": 1, // ONLINE
    })
}

fn user_object(user_id: i64, sender: Option<&Sender>) -> serde_json::Value {
    let mut user = serde_json::json!({
        "id": user_id.to_string(),
    });
    if let Some(s) = sender {
        if !s.nickname.is_empty() {
            user["name"] = serde_json::json!(&s.nickname);
        }
        if let Some(ref card) = s.card {
            if !card.is_empty() {
                user["nick"] = serde_json::json!(card);
            }
        }
    }
    user
}

fn channel_object(msg: &MessageEvent) -> serde_json::Value {
    match msg.message_type {
        MessageType::Group => {
            let gid = msg.group_id.unwrap_or(0);
            serde_json::json!({
                "id": gid.to_string(),
                "type": 0, // TEXT
            })
        }
        MessageType::Private => {
            // Direct messages: channel id = user id.
            serde_json::json!({
                "id": msg.user_id.to_string(),
                "type": 1, // DIRECT
            })
        }
    }
}

fn guild_object(msg: &MessageEvent) -> Option<serde_json::Value> {
    match msg.message_type {
        MessageType::Group => {
            let gid = msg.group_id.unwrap_or(0);
            Some(serde_json::json!({
                "id": gid.to_string(),
            }))
        }
        MessageType::Private => None,
    }
}

fn message_event_to_satori(msg: &MessageEvent, sn: u64) -> serde_json::Value {
    let content = segments_to_satori_elements(&msg.message);

    let mut body = serde_json::json!({
        "sn": sn,
        "type": "message-created",
        "timestamp": msg.time.timestamp_millis(),
        "login": login_object(msg.self_id),
        "channel": channel_object(msg),
        "user": user_object(msg.user_id, Some(&msg.sender)),
        "message": {
            "id": msg.message_id.to_string(),
            "content": content,
            "user": user_object(msg.user_id, Some(&msg.sender)),
            "created_at": msg.time.timestamp_millis(),
        },
    });

    if let Some(guild) = guild_object(msg) {
        body["guild"] = guild;
        // Also add member object for group messages.
        let mut member = serde_json::Map::new();
        if let Some(ref card) = msg.sender.card {
            if !card.is_empty() {
                member.insert("nick".to_string(), serde_json::json!(card));
            }
        }
        body["member"] = serde_json::Value::Object(member);
    }

    body
}

fn notice_event_to_satori(evt: &NoticeEvent, sn: u64) -> serde_json::Value {
    // Map OneBot notice types to Satori event types.
    let satori_type = match evt.notice_type.as_str() {
        "group_increase" => "guild-member-added",
        "group_decrease" => "guild-member-removed",
        "group_ban" => "guild-member-updated",
        "group_recall" => "message-deleted",
        "friend_recall" => "message-deleted",
        "group_admin" => "guild-member-updated",
        "notify" => "interaction/button", // poke etc.
        _ => "internal",
    };

    serde_json::json!({
        "sn": sn,
        "type": satori_type,
        "timestamp": evt.time.timestamp_millis(),
        "login": login_object(evt.self_id),
    })
}

fn request_event_to_satori(evt: &RequestEvent, sn: u64) -> serde_json::Value {
    let satori_type = match evt.request_type.as_str() {
        "friend" => "friend-request",
        "group" => "guild-member-request",
        _ => "internal",
    };

    serde_json::json!({
        "sn": sn,
        "type": satori_type,
        "timestamp": evt.time.timestamp_millis(),
        "login": login_object(evt.self_id),
    })
}

fn meta_event_to_satori(evt: &MetaEvent, sn: u64) -> serde_json::Value {
    let satori_type = match evt.meta_event_type.as_str() {
        "lifecycle" => "login-updated",
        "heartbeat" => "internal",
        _ => "internal",
    };

    serde_json::json!({
        "sn": sn,
        "type": satori_type,
        "timestamp": evt.time.timestamp_millis(),
        "login": login_object(evt.self_id),
    })
}

// ---------------------------------------------------------------------------
// Message segments → Satori element encoding
// ---------------------------------------------------------------------------

/// Convert internal message segments to Satori's HTML-like element string.
///
/// Satori uses an XML/HTML-like encoding:
/// - Plain text (with `&`, `<`, `>` escaped)
/// - `<at id="..." />` for mentions
/// - `<img src="..." />` for images
/// - `<audio src="..." />` for voice
/// - `<video src="..." />` for video
/// - `<file src="..." />` for files
/// - `<quote><message id="..." /></quote>` for replies
pub fn segments_to_satori_elements(segments: &[MessageSegment]) -> String {
    let mut out = String::new();
    for seg in segments {
        match seg {
            MessageSegment::Text { text } => {
                out.push_str(&escape_satori_text(text));
            }
            MessageSegment::At { qq } => {
                if qq == "all" {
                    out.push_str(r#"<at type="all" />"#);
                } else {
                    out.push_str(&format!(r#"<at id="{}" />"#, escape_attr(qq)));
                }
            }
            MessageSegment::Image { file, url } => {
                let src = url.as_deref().unwrap_or(file);
                out.push_str(&format!(r#"<img src="{}" />"#, escape_attr(src)));
            }
            MessageSegment::Face { id } => {
                out.push_str(&format!(r#"<emoji id="{}" />"#, escape_attr(id)));
            }
            MessageSegment::Reply { id } => {
                out.push_str(&format!(
                    r#"<quote><message id="{}" /></quote>"#,
                    escape_attr(id)
                ));
            }
            MessageSegment::Record { file, url } => {
                let src = url.as_deref().unwrap_or(file);
                out.push_str(&format!(r#"<audio src="{}" />"#, escape_attr(src)));
            }
            MessageSegment::Video { file, url } => {
                let src = url.as_deref().unwrap_or(file);
                out.push_str(&format!(r#"<video src="{}" />"#, escape_attr(src)));
            }
            MessageSegment::Forward { id } => {
                out.push_str(&format!(r#"<message id="{}" forward />"#, escape_attr(id)));
            }
            MessageSegment::Json { data } | MessageSegment::Xml { data } => {
                out.push_str(&escape_satori_text(data));
            }
            MessageSegment::Poke { poke_type, id } => {
                out.push_str(&format!(
                    r#"<qq:poke type="{}" id="{}" />"#,
                    escape_attr(poke_type),
                    escape_attr(id)
                ));
            }
            MessageSegment::Unknown => {}
        }
    }
    out
}

/// Parse Satori element string back into internal message segments.
///
/// This is a simplified parser that handles the common elements.
pub fn parse_satori_elements(content: &str) -> Vec<MessageSegment> {
    let mut segments = Vec::new();
    let mut chars = content.chars().peekable();
    let mut text_buf = String::new();

    while let Some(&ch) = chars.peek() {
        if ch == '<' {
            // Flush accumulated text.
            if !text_buf.is_empty() {
                segments.push(MessageSegment::Text {
                    text: unescape_satori_text(&text_buf),
                });
                text_buf.clear();
            }
            // Read the entire tag.
            let tag = read_tag(&mut chars);
            if let Some(seg) = parse_satori_tag(&tag) {
                segments.push(seg);
            }
        } else if ch == '&' {
            // Read entity.
            text_buf.push_str(&read_entity(&mut chars));
        } else {
            text_buf.push(ch);
            chars.next();
        }
    }

    if !text_buf.is_empty() {
        segments.push(MessageSegment::Text {
            text: unescape_satori_text(&text_buf),
        });
    }

    segments
}

/// Read a complete XML-like tag from `<` to `>`.
fn read_tag(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut tag = String::new();
    // Skip the opening '<'.
    chars.next();
    while let Some(&ch) = chars.peek() {
        chars.next();
        if ch == '>' {
            break;
        }
        tag.push(ch);
    }
    tag
}

/// Read an HTML entity starting with `&`.
fn read_entity(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut entity = String::new();
    entity.push('&');
    chars.next(); // skip '&'
    while let Some(&ch) = chars.peek() {
        entity.push(ch);
        chars.next();
        if ch == ';' {
            break;
        }
    }
    // Return the raw entity — unescape later.
    entity
}

/// Parse a single Satori element tag into a message segment.
fn parse_satori_tag(tag: &str) -> Option<MessageSegment> {
    let tag = tag.trim().trim_end_matches('/').trim();

    if tag.starts_with("at") {
        let attrs = parse_attrs(tag.strip_prefix("at").unwrap_or(""));
        if let Some(at_type) = attrs.get("type") {
            if at_type == "all" || at_type == "here" {
                return Some(MessageSegment::At {
                    qq: at_type.clone(),
                });
            }
        }
        if let Some(id) = attrs.get("id") {
            return Some(MessageSegment::At { qq: id.clone() });
        }
        return None;
    }

    if tag.starts_with("img") {
        let attrs = parse_attrs(tag.strip_prefix("img").unwrap_or(""));
        if let Some(src) = attrs.get("src") {
            return Some(MessageSegment::Image {
                file: src.clone(),
                url: Some(src.clone()),
            });
        }
        return None;
    }

    if tag.starts_with("audio") {
        let attrs = parse_attrs(tag.strip_prefix("audio").unwrap_or(""));
        if let Some(src) = attrs.get("src") {
            return Some(MessageSegment::Record {
                file: src.clone(),
                url: Some(src.clone()),
            });
        }
        return None;
    }

    if tag.starts_with("video") {
        let attrs = parse_attrs(tag.strip_prefix("video").unwrap_or(""));
        if let Some(src) = attrs.get("src") {
            return Some(MessageSegment::Video {
                file: src.clone(),
                url: Some(src.clone()),
            });
        }
        return None;
    }

    if tag.starts_with("emoji") {
        let attrs = parse_attrs(tag.strip_prefix("emoji").unwrap_or(""));
        if let Some(id) = attrs.get("id") {
            return Some(MessageSegment::Face { id: id.clone() });
        }
        return None;
    }

    // <message id="..." forward /> → Forward segment
    if tag.starts_with("message") {
        let attrs = parse_attrs(tag.strip_prefix("message").unwrap_or(""));
        if attrs.contains_key("forward") || tag.contains("forward") {
            if let Some(id) = attrs.get("id") {
                return Some(MessageSegment::Forward { id: id.clone() });
            }
        }
        // Quoted message (inside <quote>): <message id="..." />
        if let Some(id) = attrs.get("id") {
            return Some(MessageSegment::Reply { id: id.clone() });
        }
        return None;
    }

    // Skip closing tags and unrecognized elements.
    None
}

/// Parse simple XML-like attributes from a string like ` id="123" name="foo"`.
fn parse_attrs(s: &str) -> std::collections::HashMap<String, String> {
    let mut attrs = std::collections::HashMap::new();
    let s = s.trim();
    let mut chars = s.chars().peekable();

    while chars.peek().is_some() {
        // Skip whitespace.
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }

        // Read attribute name.
        let mut name = String::new();
        while chars
            .peek()
            .is_some_and(|c| *c != '=' && !c.is_whitespace() && *c != '/')
        {
            name.push(chars.next().unwrap());
        }

        if name.is_empty() {
            chars.next(); // skip unexpected char
            continue;
        }

        // Check for `=`.
        if chars.peek() == Some(&'=') {
            chars.next(); // skip '='
            // Read value.
            let quote = chars.peek().copied();
            if quote == Some('"') || quote == Some('\'') {
                chars.next(); // skip opening quote
                let mut value = String::new();
                while chars.peek().is_some_and(|c| *c != quote.unwrap()) {
                    value.push(chars.next().unwrap());
                }
                chars.next(); // skip closing quote
                attrs.insert(name, unescape_attr(&value));
            } else {
                // Unquoted value.
                let mut value = String::new();
                while chars
                    .peek()
                    .is_some_and(|c| !c.is_whitespace() && *c != '/')
                {
                    value.push(chars.next().unwrap());
                }
                attrs.insert(name, value);
            }
        } else {
            // Boolean attribute (e.g., `forward`).
            attrs.insert(name, String::new());
        }
    }

    attrs
}

// ---------------------------------------------------------------------------
// Satori API → internal ApiRequest translation
// ---------------------------------------------------------------------------

/// Parse a Satori API call into an internal [`ApiRequest`].
///
/// `resource_method` is the `{resource}.{method}` path, e.g. `message.create`.
/// `body` is the JSON request body.
/// `self_id` is extracted from the `Satori-User-ID` header.
pub fn parse_satori_api(
    resource_method: &str,
    body: serde_json::Value,
    self_id: Option<i64>,
) -> Result<ApiRequest, GatewayError> {
    let (action, params) = translate_satori_api(resource_method, body)?;

    Ok(ApiRequest {
        action,
        params,
        echo: None,
        self_id,
    })
}

/// Translate a Satori `resource.method` and body into a v11-compatible action + params.
fn translate_satori_api(
    resource_method: &str,
    body: serde_json::Value,
) -> Result<(String, serde_json::Value), GatewayError> {
    match resource_method {
        "message.create" => {
            // Satori: { channel_id, content } → v11: send_msg
            let channel_id = body
                .get("channel_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let content = body
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let segments = parse_satori_elements(&content);
            let message: Vec<serde_json::Value> = segments
                .iter()
                .map(|s| serde_json::to_value(s).unwrap_or_default())
                .collect();

            // Determine if this is a group or private message.
            // In our QQ context, numeric IDs that look like group IDs go to send_group_msg.
            // The Satori spec doesn't distinguish — the channel_id determines it.
            // For simplicity, we try group first; if channel type info is available, use it.
            let mut params = serde_json::Map::new();
            if let Ok(id) = channel_id.parse::<i64>() {
                params.insert("group_id".to_string(), serde_json::json!(id));
                params.insert("message".to_string(), serde_json::Value::Array(message));
                Ok((
                    "send_group_msg".to_string(),
                    serde_json::Value::Object(params),
                ))
            } else {
                params.insert("message".to_string(), serde_json::Value::Array(message));
                Ok(("send_msg".to_string(), serde_json::Value::Object(params)))
            }
        }
        "message.get" => {
            let mut params = serde_json::Map::new();
            if let Some(msg_id) = body.get("message_id").and_then(|v| v.as_str()) {
                if let Ok(id) = msg_id.parse::<i64>() {
                    params.insert("message_id".to_string(), serde_json::json!(id));
                }
            }
            Ok(("get_msg".to_string(), serde_json::Value::Object(params)))
        }
        "message.delete" => {
            let mut params = serde_json::Map::new();
            if let Some(msg_id) = body.get("message_id").and_then(|v| v.as_str()) {
                if let Ok(id) = msg_id.parse::<i64>() {
                    params.insert("message_id".to_string(), serde_json::json!(id));
                }
            }
            Ok(("delete_msg".to_string(), serde_json::Value::Object(params)))
        }
        "channel.get" => {
            let mut params = serde_json::Map::new();
            if let Some(ch_id) = body.get("channel_id").and_then(|v| v.as_str()) {
                if let Ok(id) = ch_id.parse::<i64>() {
                    params.insert("group_id".to_string(), serde_json::json!(id));
                }
            }
            Ok((
                "get_group_info".to_string(),
                serde_json::Value::Object(params),
            ))
        }
        "channel.list" => {
            // Not directly supported — return group list.
            Ok((
                "get_group_list".to_string(),
                serde_json::Value::Object(Default::default()),
            ))
        }
        "guild.get" => {
            let mut params = serde_json::Map::new();
            if let Some(gid) = body.get("guild_id").and_then(|v| v.as_str()) {
                if let Ok(id) = gid.parse::<i64>() {
                    params.insert("group_id".to_string(), serde_json::json!(id));
                }
            }
            Ok((
                "get_group_info".to_string(),
                serde_json::Value::Object(params),
            ))
        }
        "guild.list" => Ok((
            "get_group_list".to_string(),
            serde_json::Value::Object(Default::default()),
        )),
        "guild.member.get" => {
            let mut params = serde_json::Map::new();
            if let Some(gid) = body.get("guild_id").and_then(|v| v.as_str()) {
                if let Ok(id) = gid.parse::<i64>() {
                    params.insert("group_id".to_string(), serde_json::json!(id));
                }
            }
            if let Some(uid) = body.get("user_id").and_then(|v| v.as_str()) {
                if let Ok(id) = uid.parse::<i64>() {
                    params.insert("user_id".to_string(), serde_json::json!(id));
                }
            }
            Ok((
                "get_group_member_info".to_string(),
                serde_json::Value::Object(params),
            ))
        }
        "guild.member.list" => {
            let mut params = serde_json::Map::new();
            if let Some(gid) = body.get("guild_id").and_then(|v| v.as_str()) {
                if let Ok(id) = gid.parse::<i64>() {
                    params.insert("group_id".to_string(), serde_json::json!(id));
                }
            }
            Ok((
                "get_group_member_list".to_string(),
                serde_json::Value::Object(params),
            ))
        }
        "guild.member.kick" => {
            let mut params = serde_json::Map::new();
            if let Some(gid) = body.get("guild_id").and_then(|v| v.as_str()) {
                if let Ok(id) = gid.parse::<i64>() {
                    params.insert("group_id".to_string(), serde_json::json!(id));
                }
            }
            if let Some(uid) = body.get("user_id").and_then(|v| v.as_str()) {
                if let Ok(id) = uid.parse::<i64>() {
                    params.insert("user_id".to_string(), serde_json::json!(id));
                }
            }
            Ok((
                "set_group_kick".to_string(),
                serde_json::Value::Object(params),
            ))
        }
        "guild.member.mute" => {
            let mut params = serde_json::Map::new();
            if let Some(gid) = body.get("guild_id").and_then(|v| v.as_str()) {
                if let Ok(id) = gid.parse::<i64>() {
                    params.insert("group_id".to_string(), serde_json::json!(id));
                }
            }
            if let Some(uid) = body.get("user_id").and_then(|v| v.as_str()) {
                if let Ok(id) = uid.parse::<i64>() {
                    params.insert("user_id".to_string(), serde_json::json!(id));
                }
            }
            if let Some(dur) = body.get("duration").and_then(|v| v.as_u64()) {
                // Satori duration is in ms, OneBot v11 expects seconds.
                params.insert("duration".to_string(), serde_json::json!(dur / 1000));
            }
            Ok((
                "set_group_ban".to_string(),
                serde_json::Value::Object(params),
            ))
        }
        "user.get" => {
            let mut params = serde_json::Map::new();
            if let Some(uid) = body.get("user_id").and_then(|v| v.as_str()) {
                if let Ok(id) = uid.parse::<i64>() {
                    params.insert("user_id".to_string(), serde_json::json!(id));
                }
            }
            Ok((
                "get_stranger_info".to_string(),
                serde_json::Value::Object(params),
            ))
        }
        "friend.list" => Ok((
            "get_friend_list".to_string(),
            serde_json::Value::Object(Default::default()),
        )),
        "login.get" => Ok((
            "get_login_info".to_string(),
            serde_json::Value::Object(Default::default()),
        )),
        _ => Err(GatewayError::Internal(format!(
            "unsupported Satori API: {resource_method}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// v11 API response → Satori response translation
// ---------------------------------------------------------------------------

/// Translate a v11 API response to Satori format for a given resource.method.
pub fn translate_response(resource_method: &str, resp: ApiResponse) -> serde_json::Value {
    if resp.retcode != 0 {
        return resp.data;
    }

    match resource_method {
        "message.create" => {
            // v11 returns { message_id: int }
            // Satori expects an array of Message objects.
            let msg_id = resp
                .data
                .get("message_id")
                .map(|v| match v {
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default();

            serde_json::json!([{
                "id": msg_id,
                "content": "",
            }])
        }
        "login.get" => {
            // v11: { user_id, nickname } → Satori Login object
            let user_id = resp
                .data
                .get("user_id")
                .map(|v| match v {
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default();
            let nickname = resp
                .data
                .get("nickname")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            serde_json::json!({
                "sn": 0,
                "platform": "qq",
                "user": {
                    "id": user_id,
                    "name": nickname,
                },
                "status": 1,
            })
        }
        "user.get" => {
            // v11: { user_id, nickname, sex, age } → Satori User object
            let user_id = resp
                .data
                .get("user_id")
                .map(|v| match v {
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default();
            let nickname = resp
                .data
                .get("nickname")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            serde_json::json!({
                "id": user_id,
                "name": nickname,
            })
        }
        "channel.get" | "guild.get" => {
            let group_id = resp
                .data
                .get("group_id")
                .map(|v| match v {
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default();
            let name = resp
                .data
                .get("group_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if resource_method == "channel.get" {
                serde_json::json!({
                    "id": group_id,
                    "type": 0,
                    "name": name,
                })
            } else {
                serde_json::json!({
                    "id": group_id,
                    "name": name,
                })
            }
        }
        "channel.list" | "guild.list" => {
            // v11 returns an array of group info objects.
            if let Some(arr) = resp.data.as_array() {
                let items: Vec<serde_json::Value> = arr
                    .iter()
                    .map(|g| {
                        let id = g
                            .get("group_id")
                            .map(|v| match v {
                                serde_json::Value::Number(n) => n.to_string(),
                                serde_json::Value::String(s) => s.clone(),
                                other => other.to_string(),
                            })
                            .unwrap_or_default();
                        let name = g
                            .get("group_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();

                        if resource_method == "channel.list" {
                            serde_json::json!({ "id": id, "type": 0, "name": name })
                        } else {
                            serde_json::json!({ "id": id, "name": name })
                        }
                    })
                    .collect();
                serde_json::json!({ "data": items })
            } else {
                serde_json::json!({ "data": [] })
            }
        }
        "guild.member.get" => {
            let user_id = resp
                .data
                .get("user_id")
                .map(|v| match v {
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default();
            let nickname = resp
                .data
                .get("nickname")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let card = resp.data.get("card").and_then(|v| v.as_str()).unwrap_or("");

            serde_json::json!({
                "user": {
                    "id": user_id,
                    "name": nickname,
                },
                "nick": if card.is_empty() { nickname } else { card },
            })
        }
        "guild.member.list" => {
            if let Some(arr) = resp.data.as_array() {
                let items: Vec<serde_json::Value> = arr
                    .iter()
                    .map(|m| {
                        let uid = m
                            .get("user_id")
                            .map(|v| match v {
                                serde_json::Value::Number(n) => n.to_string(),
                                serde_json::Value::String(s) => s.clone(),
                                other => other.to_string(),
                            })
                            .unwrap_or_default();
                        let nick = m.get("nickname").and_then(|v| v.as_str()).unwrap_or("");
                        let card = m.get("card").and_then(|v| v.as_str()).unwrap_or("");

                        serde_json::json!({
                            "user": { "id": uid, "name": nick },
                            "nick": if card.is_empty() { nick } else { card },
                        })
                    })
                    .collect();
                serde_json::json!({ "data": items })
            } else {
                serde_json::json!({ "data": [] })
            }
        }
        "friend.list" => {
            if let Some(arr) = resp.data.as_array() {
                let items: Vec<serde_json::Value> = arr
                    .iter()
                    .map(|f| {
                        let uid = f
                            .get("user_id")
                            .map(|v| match v {
                                serde_json::Value::Number(n) => n.to_string(),
                                serde_json::Value::String(s) => s.clone(),
                                other => other.to_string(),
                            })
                            .unwrap_or_default();
                        let nick = f.get("nickname").and_then(|v| v.as_str()).unwrap_or("");
                        let remark = f.get("remark").and_then(|v| v.as_str()).unwrap_or("");

                        serde_json::json!({
                            "id": uid,
                            "name": nick,
                            "nick": if remark.is_empty() { nick } else { remark },
                        })
                    })
                    .collect();
                serde_json::json!({ "data": items })
            } else {
                serde_json::json!({ "data": [] })
            }
        }
        // Pass through raw data for actions we don't specifically translate.
        _ => resp.data,
    }
}

// ---------------------------------------------------------------------------
// Text escaping helpers for Satori element encoding
// ---------------------------------------------------------------------------

fn escape_satori_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn unescape_satori_text(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn unescape_attr(s: &str) -> String {
    unescape_satori_text(s)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    fn sample_message_event() -> Event {
        Event::Message(Box::new(MessageEvent {
            id: Uuid::nil(),
            time: Utc.timestamp_opt(1700000000, 0).unwrap(),
            self_id: 123456,
            message_type: MessageType::Group,
            sub_type: "normal".to_string(),
            message_id: 99,
            user_id: 789,
            group_id: Some(111),
            message: vec![
                MessageSegment::Text {
                    text: "hello ".to_string(),
                },
                MessageSegment::At {
                    qq: "456".to_string(),
                },
            ],
            raw_message: "hello [CQ:at,qq=456]".to_string(),
            sender: Sender {
                user_id: 789,
                nickname: "TestUser".to_string(),
                card: Some("CardName".to_string()),
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

    #[test]
    fn event_to_satori_message_created() {
        let event = sample_message_event();
        let json = event_to_satori(&event, 42);

        assert_eq!(json["sn"], 42);
        assert_eq!(json["type"], "message-created");
        assert_eq!(json["login"]["platform"], "qq");
        assert_eq!(json["channel"]["id"], "111");
        assert_eq!(json["channel"]["type"], 0);
        assert_eq!(json["user"]["id"], "789");
        assert_eq!(json["message"]["id"], "99");
        assert!(json["guild"]["id"].as_str().is_some());

        // Content should be Satori elements.
        let content = json["message"]["content"].as_str().unwrap();
        assert!(content.contains("hello "));
        assert!(content.contains(r#"<at id="456" />"#));
    }

    #[test]
    fn event_signal_wrapping() {
        let event = sample_message_event();
        let signal = event_to_signal(&event, 1);

        assert_eq!(signal["op"], 0);
        assert!(signal["body"]["sn"].is_number());
    }

    #[test]
    fn segments_to_elements_roundtrip() {
        let segments = vec![
            MessageSegment::Text {
                text: "hi <world>".to_string(),
            },
            MessageSegment::At {
                qq: "123".to_string(),
            },
            MessageSegment::Image {
                file: "https://example.com/a.png".to_string(),
                url: Some("https://example.com/a.png".to_string()),
            },
        ];

        let elements = segments_to_satori_elements(&segments);
        assert!(elements.contains("hi &lt;world&gt;"));
        assert!(elements.contains(r#"<at id="123" />"#));
        assert!(elements.contains(r#"<img src="https://example.com/a.png" />"#));

        // Parse back.
        let parsed = parse_satori_elements(&elements);
        assert_eq!(parsed.len(), 3);
        assert_eq!(
            parsed[0],
            MessageSegment::Text {
                text: "hi <world>".to_string()
            }
        );
        assert_eq!(
            parsed[1],
            MessageSegment::At {
                qq: "123".to_string()
            }
        );
        match &parsed[2] {
            MessageSegment::Image { file, .. } => {
                assert_eq!(file, "https://example.com/a.png");
            }
            _ => panic!("expected Image segment"),
        }
    }

    #[test]
    fn parse_at_all() {
        let content = r#"<at type="all" />"#;
        let segs = parse_satori_elements(content);
        assert_eq!(segs.len(), 1);
        assert_eq!(
            segs[0],
            MessageSegment::At {
                qq: "all".to_string()
            }
        );
    }

    #[test]
    fn translate_message_create() {
        let body = serde_json::json!({
            "channel_id": "12345",
            "content": "hello <at id=\"789\" />",
        });

        let req = parse_satori_api("message.create", body, Some(123)).unwrap();
        assert_eq!(req.action, "send_group_msg");
        assert_eq!(req.params["group_id"], 12345);
        assert_eq!(req.self_id, Some(123));
    }

    #[test]
    fn translate_login_get() {
        let body = serde_json::json!({});
        let req = parse_satori_api("login.get", body, None).unwrap();
        assert_eq!(req.action, "get_login_info");
    }

    #[test]
    fn translate_response_login() {
        let resp = ApiResponse::ok(serde_json::json!({
            "user_id": 123456,
            "nickname": "Bot",
        }));

        let result = translate_response("login.get", resp);
        assert_eq!(result["platform"], "qq");
        assert_eq!(result["user"]["id"], "123456");
        assert_eq!(result["user"]["name"], "Bot");
        assert_eq!(result["status"], 1);
    }
}
