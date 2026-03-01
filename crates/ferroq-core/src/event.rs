//! Unified internal event types.
//!
//! These types represent the superset of events from all supported backends
//! (Lagrange, NapCat, Official API) normalized into a common format.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::message::MessageSegment;

/// A unique event identifier.
pub type EventId = Uuid;

/// Top-level event enum dispatched through the EventBus.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "post_type")]
pub enum Event {
    /// A message event (group, private, etc.)
    #[serde(rename = "message")]
    Message(Box<MessageEvent>),

    /// A notice event (group member change, recall, poke, etc.)
    #[serde(rename = "notice")]
    Notice(NoticeEvent),

    /// A request event (friend request, group join request, etc.)
    #[serde(rename = "request")]
    Request(RequestEvent),

    /// A meta event (lifecycle, heartbeat, etc.)
    #[serde(rename = "meta_event")]
    Meta(MetaEvent),
}

impl Event {
    /// Returns the self_id (bot account) that generated this event.
    pub fn self_id(&self) -> i64 {
        match self {
            Event::Message(e) => e.self_id,
            Event::Notice(e) => e.self_id,
            Event::Request(e) => e.self_id,
            Event::Meta(e) => e.self_id,
        }
    }

    /// Returns the timestamp of this event.
    pub fn time(&self) -> DateTime<Utc> {
        match self {
            Event::Message(e) => e.time,
            Event::Notice(e) => e.time,
            Event::Request(e) => e.time,
            Event::Meta(e) => e.time,
        }
    }
}

/// A message event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEvent {
    pub id: EventId,
    pub time: DateTime<Utc>,
    pub self_id: i64,
    pub message_type: MessageType,
    pub sub_type: String,
    pub message_id: i64,
    pub user_id: i64,
    pub group_id: Option<i64>,
    pub message: Vec<MessageSegment>,
    pub raw_message: String,
    pub sender: Sender,
    pub font: i32,
}

/// Message type: private or group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    Private,
    Group,
}

/// Message sender information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sender {
    pub user_id: i64,
    pub nickname: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// A notice event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoticeEvent {
    pub id: EventId,
    pub time: DateTime<Utc>,
    pub self_id: i64,
    pub notice_type: String,
    pub sub_type: String,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// A request event (friend request, group invite, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestEvent {
    pub id: EventId,
    pub time: DateTime<Utc>,
    pub self_id: i64,
    pub request_type: String,
    pub sub_type: String,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// A meta event (lifecycle, heartbeat).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaEvent {
    pub id: EventId,
    pub time: DateTime<Utc>,
    pub self_id: i64,
    pub meta_event_type: String,
    pub sub_type: String,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_roundtrip_json() {
        let event = Event::Meta(MetaEvent {
            id: Uuid::new_v4(),
            time: Utc::now(),
            self_id: 123456,
            meta_event_type: "heartbeat".to_string(),
            sub_type: String::new(),
            extra: serde_json::json!({"status": {"online": true}}),
        });

        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("meta_event"));
    }
}
