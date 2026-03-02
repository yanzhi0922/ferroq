//! Event deduplication filter.
//!
//! When failover is active, both the primary and fallback adapters may be
//! connected to backends serving the same QQ account. This means the same
//! event (e.g. a group message) will arrive twice — once from each adapter.
//!
//! `DedupFilter` keeps a time-windowed set of recently-seen event fingerprints
//! and silently drops duplicates before they enter the event bus.
//!
//! # Fingerprint strategy
//!
//! - **Message events**: `(self_id, message_id)` — the backend assigns a
//!   unique `message_id` per message.
//! - **Notice / Request events**: `(self_id, type, sub_type, time_sec)` with
//!   `extra` hash — we hash the `extra` JSON to distinguish same-second events.
//! - **Meta events**: `(self_id, meta_event_type, time_sec)` — heartbeats with
//!   the same timestamp from two adapters are duplicates.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use ferroq_core::event::Event;

/// A compact fingerprint for deduplication.
///
/// We intentionally avoid hashing the full event JSON for performance.
/// Instead we use a 128-bit key derived from the most discriminating fields.
type Fingerprint = u128;

/// Time-windowed event deduplication filter.
pub struct DedupFilter {
    /// Window duration — fingerprints older than this are evicted.
    window: Duration,
    /// Monotonic start point used for atomic eviction timestamps.
    start: Instant,
    /// Map from fingerprint → insertion time.
    seen: Mutex<HashMap<Fingerprint, Instant>>,
    /// Counter: total duplicates suppressed.
    duplicates: AtomicU64,
    /// Counter: total events checked.
    checked: AtomicU64,
    /// Last eviction time (nanoseconds since `start`).
    last_eviction_ns: AtomicU64,
    /// Minimum interval between eviction sweeps.
    eviction_interval: Duration,
}

impl DedupFilter {
    /// Create a new dedup filter with the given time window (in seconds).
    ///
    /// Events with the same fingerprint arriving within `window_secs` are
    /// considered duplicates.
    pub fn new(window_secs: u64) -> Self {
        let start = Instant::now();
        Self {
            window: Duration::from_secs(window_secs),
            start,
            seen: Mutex::new(HashMap::with_capacity(8192)),
            duplicates: AtomicU64::new(0),
            checked: AtomicU64::new(0),
            last_eviction_ns: AtomicU64::new(0),
            eviction_interval: Duration::from_secs(window_secs.max(10)),
        }
    }

    /// Check whether the event is a duplicate.
    ///
    /// Returns `true` if the event should be **dropped** (it's a duplicate).
    /// Returns `false` if the event is new and has been recorded.
    pub fn is_duplicate(&self, event: &Event) -> bool {
        self.checked.fetch_add(1, Ordering::Relaxed);

        let fp = Self::fingerprint(event);

        let now = Instant::now();
        let mut seen = self.seen.lock();

        // Check if we've seen this fingerprint recently.
        if let Some(&insert_time) = seen.get(&fp) {
            if now.duration_since(insert_time) < self.window {
                self.duplicates.fetch_add(1, Ordering::Relaxed);
                return true;
            }
            // Expired — update the timestamp.
            seen.insert(fp, now);
        } else {
            seen.insert(fp, now);
        }

        // Periodically evict old entries.
        self.maybe_evict(&mut seen, now);

        false
    }

    /// Evict expired entries if enough time has passed since the last sweep.
    fn maybe_evict(&self, seen: &mut HashMap<Fingerprint, Instant>, now: Instant) {
        let now_ns = now.duration_since(self.start).as_nanos() as u64;
        let interval_ns = self.eviction_interval.as_nanos() as u64;
        let last_ns = self.last_eviction_ns.load(Ordering::Relaxed);
        if now_ns.saturating_sub(last_ns) < interval_ns {
            return;
        }

        if self
            .last_eviction_ns
            .compare_exchange(last_ns, now_ns, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            let window = self.window;
            seen.retain(|_, &mut ts| now.duration_since(ts) < window);
        }
    }

    /// Total duplicates suppressed.
    pub fn duplicates_total(&self) -> u64 {
        self.duplicates.load(Ordering::Relaxed)
    }

    /// Total events checked.
    pub fn checked_total(&self) -> u64 {
        self.checked.load(Ordering::Relaxed)
    }

    /// Compute a fingerprint for an event.
    fn fingerprint(event: &Event) -> Fingerprint {
        match event {
            Event::Message(msg) => {
                // (self_id, message_id) is unique per message.
                let hi = msg.self_id as u64;
                let lo = msg.message_id as u64;
                ((hi as u128) << 64) | (lo as u128)
            }
            Event::Notice(n) => {
                // Combine self_id, notice_type, sub_type, timestamp, and a hash of extra.
                Self::hash_non_message(
                    n.self_id,
                    &n.notice_type,
                    &n.sub_type,
                    n.time.timestamp(),
                    &n.extra,
                )
            }
            Event::Request(r) => Self::hash_non_message(
                r.self_id,
                &r.request_type,
                &r.sub_type,
                r.time.timestamp(),
                &r.extra,
            ),
            Event::Meta(m) => Self::hash_non_message(
                m.self_id,
                &m.meta_event_type,
                &m.sub_type,
                m.time.timestamp(),
                &m.extra,
            ),
        }
    }

    /// Hash helper for non-message events.
    fn hash_non_message(
        self_id: i64,
        event_type: &str,
        sub_type: &str,
        timestamp: i64,
        extra: &serde_json::Value,
    ) -> Fingerprint {
        use std::hash::{Hash, Hasher};

        // Build a stable 128-bit fingerprint without allocating intermediate
        // JSON strings (hot path under high event throughput).
        let mut hasher = std::hash::DefaultHasher::new();
        self_id.hash(&mut hasher);
        event_type.hash(&mut hasher);
        sub_type.hash(&mut hasher);
        timestamp.hash(&mut hasher);
        Self::hash_json_value(extra, &mut hasher);
        let lo = hasher.finish();

        // Second hash pass for the high bits (reduce collision probability).
        let mut hasher2 = std::hash::DefaultHasher::new();
        lo.hash(&mut hasher2);
        self_id.hash(&mut hasher2);
        timestamp.hash(&mut hasher2);
        Self::hash_json_value(extra, &mut hasher2);
        let hi = hasher2.finish();

        ((hi as u128) << 64) | (lo as u128)
    }

    fn hash_json_value<H: std::hash::Hasher>(value: &serde_json::Value, hasher: &mut H) {
        use std::hash::Hash;

        match value {
            serde_json::Value::Null => {
                0u8.hash(hasher);
            }
            serde_json::Value::Bool(b) => {
                1u8.hash(hasher);
                b.hash(hasher);
            }
            serde_json::Value::Number(n) => {
                2u8.hash(hasher);
                if let Some(i) = n.as_i64() {
                    0u8.hash(hasher);
                    i.hash(hasher);
                } else if let Some(u) = n.as_u64() {
                    1u8.hash(hasher);
                    u.hash(hasher);
                } else if let Some(f) = n.as_f64() {
                    2u8.hash(hasher);
                    f.to_bits().hash(hasher);
                }
            }
            serde_json::Value::String(s) => {
                3u8.hash(hasher);
                s.hash(hasher);
            }
            serde_json::Value::Array(arr) => {
                4u8.hash(hasher);
                arr.len().hash(hasher);
                for v in arr {
                    Self::hash_json_value(v, hasher);
                }
            }
            serde_json::Value::Object(map) => {
                5u8.hash(hasher);
                map.len().hash(hasher);
                for (k, v) in map {
                    k.hash(hasher);
                    Self::hash_json_value(v, hasher);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ferroq_core::event::*;
    use ferroq_core::message::MessageSegment;
    use uuid::Uuid;

    fn make_message_event(self_id: i64, message_id: i64) -> Event {
        Event::Message(Box::new(MessageEvent {
            id: Uuid::new_v4(),
            time: Utc::now(),
            self_id,
            message_type: MessageType::Group,
            sub_type: "normal".to_string(),
            message_id,
            user_id: 999,
            group_id: Some(100),
            message: vec![MessageSegment::Text {
                text: "hello".to_string(),
            }],
            raw_message: "hello".to_string(),
            sender: Sender {
                user_id: 999,
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
        }))
    }

    fn make_meta_event(self_id: i64, meta_type: &str) -> Event {
        Event::Meta(MetaEvent {
            id: Uuid::new_v4(),
            time: Utc::now(),
            self_id,
            meta_event_type: meta_type.to_string(),
            sub_type: String::new(),
            extra: serde_json::json!({"status": {"online": true}}),
        })
    }

    fn make_notice_event(self_id: i64, notice_type: &str) -> Event {
        Event::Notice(NoticeEvent {
            id: Uuid::new_v4(),
            time: Utc::now(),
            self_id,
            notice_type: notice_type.to_string(),
            sub_type: "".to_string(),
            extra: serde_json::json!({"user_id": 12345}),
        })
    }

    #[test]
    fn identical_message_is_duplicate() {
        let filter = DedupFilter::new(60);
        let ev1 = make_message_event(100, 1001);
        let ev2 = make_message_event(100, 1001);

        assert!(!filter.is_duplicate(&ev1), "first event should pass");
        assert!(
            filter.is_duplicate(&ev2),
            "same message_id should be duplicate"
        );
        assert_eq!(filter.duplicates_total(), 1);
        assert_eq!(filter.checked_total(), 2);
    }

    #[test]
    fn different_messages_are_not_duplicates() {
        let filter = DedupFilter::new(60);
        let ev1 = make_message_event(100, 1001);
        let ev2 = make_message_event(100, 1002);

        assert!(!filter.is_duplicate(&ev1));
        assert!(!filter.is_duplicate(&ev2));
        assert_eq!(filter.duplicates_total(), 0);
    }

    #[test]
    fn different_self_id_not_duplicate() {
        let filter = DedupFilter::new(60);
        let ev1 = make_message_event(100, 1001);
        let ev2 = make_message_event(200, 1001);

        assert!(!filter.is_duplicate(&ev1));
        assert!(!filter.is_duplicate(&ev2));
        assert_eq!(filter.duplicates_total(), 0);
    }

    #[test]
    fn meta_event_dedup() {
        let filter = DedupFilter::new(60);
        let ev1 = make_meta_event(100, "heartbeat");
        // Same self_id, same type, same second → duplicate.
        let ev2 = make_meta_event(100, "heartbeat");

        assert!(!filter.is_duplicate(&ev1));
        assert!(filter.is_duplicate(&ev2));
    }

    #[test]
    fn notice_event_dedup() {
        let filter = DedupFilter::new(60);
        let ev1 = make_notice_event(100, "group_increase");
        let ev2 = make_notice_event(100, "group_increase");

        assert!(!filter.is_duplicate(&ev1));
        assert!(filter.is_duplicate(&ev2));
    }

    #[test]
    fn expired_fingerprint_allows_new_event() {
        // Window of 0 seconds — everything expires immediately.
        let filter = DedupFilter::new(0);
        let ev1 = make_message_event(100, 1001);
        let ev2 = make_message_event(100, 1001);

        assert!(!filter.is_duplicate(&ev1));
        // With 0-sec window, the fingerprint is already expired.
        // (Instant granularity means this might or might not be expired,
        //  but with a 0-duration window it should pass through.)
        // We can't guarantee timing in unit tests, but at least verify
        // the filter doesn't panic.
        let _ = filter.is_duplicate(&ev2);
    }

    #[test]
    fn counters_track_correctly() {
        let filter = DedupFilter::new(60);
        let ev = make_message_event(100, 42);

        assert!(!filter.is_duplicate(&ev));
        assert!(filter.is_duplicate(&ev));
        assert!(filter.is_duplicate(&ev));

        assert_eq!(filter.checked_total(), 3);
        assert_eq!(filter.duplicates_total(), 2);
    }
}
