//! Event bus — broadcasts events from backends to all protocol servers.

use ferroq_core::event::Event;
use tokio::sync::broadcast;
use tracing::debug;

/// Default broadcast channel capacity.
const DEFAULT_CAPACITY: usize = 4096;

/// The event bus distributes events from backend adapters to all subscribed
/// protocol servers.
#[derive(Debug)]
pub struct EventBus {
    sender: broadcast::Sender<Event>,
}

impl EventBus {
    /// Create a new event bus with the default capacity.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Create a new event bus with a specific capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Publish an event to all subscribers.
    pub fn publish(&self, event: Event) {
        let receiver_count = self.sender.receiver_count();
        if receiver_count == 0 {
            debug!("event dropped (no subscribers): self_id={}", event.self_id());
            return;
        }
        if let Err(e) = self.sender.send(event) {
            debug!("event dropped (send error): {e}");
        }
    }

    /// Subscribe to the event bus, receiving a broadcast receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    /// Number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferroq_core::event::{MetaEvent, Event};
    use chrono::Utc;
    use uuid::Uuid;

    #[tokio::test]
    async fn publish_and_receive() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        let event = Event::Meta(MetaEvent {
            id: Uuid::new_v4(),
            time: Utc::now(),
            self_id: 123,
            meta_event_type: "heartbeat".to_string(),
            sub_type: String::new(),
            extra: serde_json::json!({}),
        });

        bus.publish(event.clone());

        let received = rx.recv().await.expect("should receive event");
        assert_eq!(received.self_id(), 123);
    }
}
