//! SQLite-based message storage.
//!
//! Persists message events into an SQLite database for history, search,
//! and auditing. Runs blocking I/O on a dedicated `spawn_blocking` thread.
//!
//! Schema: one `messages` table storing the normalized message fields and
//! the raw JSON for full fidelity.

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, NaiveDateTime, Utc};
use ferroq_core::config::StorageConfig;
use ferroq_core::error::GatewayError;
use ferroq_core::event::MessageEvent;
use parking_lot::Mutex;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

/// The message store manages a SQLite connection and provides async wrappers
/// around blocking I/O via `tokio::task::spawn_blocking`.
pub struct MessageStore {
    conn: Arc<Mutex<Connection>>,
    max_days: u32,
}

/// SQL schema for the messages table and its indexes.
const SCHEMA_SQL: &str = "
    CREATE TABLE IF NOT EXISTS messages (
        id              INTEGER PRIMARY KEY AUTOINCREMENT,
        time            TEXT    NOT NULL,
        self_id         INTEGER NOT NULL,
        message_type    TEXT    NOT NULL,
        message_id      INTEGER NOT NULL,
        user_id         INTEGER NOT NULL,
        group_id        INTEGER,
        raw_message     TEXT    NOT NULL DEFAULT '',
        sender_nickname TEXT    NOT NULL DEFAULT '',
        raw_json        TEXT    NOT NULL,
        created_at      TEXT    NOT NULL DEFAULT (datetime('now'))
    );

    CREATE INDEX IF NOT EXISTS idx_messages_time ON messages(time);
    CREATE INDEX IF NOT EXISTS idx_messages_self_id ON messages(self_id);
    CREATE INDEX IF NOT EXISTS idx_messages_group_id ON messages(group_id);
    CREATE INDEX IF NOT EXISTS idx_messages_user_id ON messages(user_id);
    CREATE INDEX IF NOT EXISTS idx_messages_message_id ON messages(message_id);
";

/// A stored message row returned from queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: i64,
    pub time: DateTime<Utc>,
    pub self_id: i64,
    pub message_type: String,
    pub message_id: i64,
    pub user_id: i64,
    pub group_id: Option<i64>,
    pub raw_message: String,
    pub sender_nickname: String,
    pub raw_json: String,
}

/// Query parameters for message search.
#[derive(Debug, Default, Deserialize)]
pub struct MessageQuery {
    /// Filter by self_id (bot account).
    pub self_id: Option<i64>,
    /// Filter by group_id.
    pub group_id: Option<i64>,
    /// Filter by user_id (sender).
    pub user_id: Option<i64>,
    /// Filter by message type: "private" or "group".
    pub message_type: Option<String>,
    /// Search text in raw_message (LIKE %keyword%).
    pub keyword: Option<String>,
    /// Maximum number of results (default 50, max 500).
    pub limit: Option<u32>,
    /// Offset for pagination.
    pub offset: Option<u32>,
    /// Start time (ISO 8601).
    pub after: Option<DateTime<Utc>>,
    /// End time (ISO 8601).
    pub before: Option<DateTime<Utc>>,
}

/// Paginated message query result.
#[derive(Debug, Serialize)]
pub struct MessageQueryResult {
    pub total: u64,
    pub messages: Vec<StoredMessage>,
}

impl MessageStore {
    /// Open (or create) the message database at the given path.
    pub fn open(config: &StorageConfig) -> Result<Self, GatewayError> {
        let path = &config.path;

        // Ensure parent directory exists.
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| GatewayError::Storage(format!("cannot create directory: {e}")))?;
        }

        let conn = Connection::open(path)
            .map_err(|e| GatewayError::Storage(format!("cannot open database: {e}")))?;

        // Set pragmas for performance.
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -8000;
             PRAGMA busy_timeout = 5000;",
        )
        .map_err(|e| GatewayError::Storage(format!("pragma setup failed: {e}")))?;

        // Create table if not exists.
        conn.execute_batch(SCHEMA_SQL)
            .map_err(|e| GatewayError::Storage(format!("table creation failed: {e}")))?;

        info!(path = %path, "message store opened");

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            max_days: config.max_days,
        })
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self, GatewayError> {
        let config = StorageConfig {
            enabled: true,
            path: ":memory:".into(),
            max_days: 30,
        };
        // For in-memory, we can't use the path-based open since `:memory:` has no parent.
        let conn = Connection::open_in_memory()
            .map_err(|e| GatewayError::Storage(format!("cannot open in-memory db: {e}")))?;

        conn.execute_batch(SCHEMA_SQL)
            .map_err(|e| GatewayError::Storage(format!("table creation failed: {e}")))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            max_days: config.max_days,
        })
    }

    /// Insert a message event into the store. Runs on a blocking thread.
    pub async fn insert(&self, event: &MessageEvent) -> Result<(), GatewayError> {
        let conn = Arc::clone(&self.conn);
        let time = event.time.to_rfc3339();
        let self_id = event.self_id;
        let message_type = format!("{:?}", event.message_type).to_lowercase();
        let message_id = event.message_id;
        let user_id = event.user_id;
        let group_id = event.group_id;
        let raw_message = event.raw_message.clone();
        let sender_nickname = event.sender.nickname.clone();
        let raw_json = serde_json::to_string(event).unwrap_or_default();

        tokio::task::spawn_blocking(move || {
            let lock = conn.lock();
            lock.execute(
                "INSERT INTO messages (time, self_id, message_type, message_id, user_id,
                 group_id, raw_message, sender_nickname, raw_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    time,
                    self_id,
                    message_type,
                    message_id,
                    user_id,
                    group_id,
                    raw_message,
                    sender_nickname,
                    raw_json,
                ],
            )
            .map_err(|e| GatewayError::Storage(format!("insert failed: {e}")))?;
            Ok(())
        })
        .await
        .map_err(|e| GatewayError::Storage(format!("spawn_blocking failed: {e}")))?
    }

    /// Query messages with filters. Runs on a blocking thread.
    pub async fn query(&self, q: &MessageQuery) -> Result<MessageQueryResult, GatewayError> {
        let conn = Arc::clone(&self.conn);
        let q = q.clone_for_blocking();

        tokio::task::spawn_blocking(move || {
            let lock = conn.lock();

            let mut conditions = Vec::new();
            let mut bind_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

            if let Some(self_id) = q.self_id {
                conditions.push("self_id = ?");
                bind_values.push(Box::new(self_id));
            }
            if let Some(group_id) = q.group_id {
                conditions.push("group_id = ?");
                bind_values.push(Box::new(group_id));
            }
            if let Some(user_id) = q.user_id {
                conditions.push("user_id = ?");
                bind_values.push(Box::new(user_id));
            }
            if let Some(ref mt) = q.message_type {
                conditions.push("message_type = ?");
                bind_values.push(Box::new(mt.clone()));
            }
            if let Some(ref kw) = q.keyword {
                conditions.push("raw_message LIKE ?");
                bind_values.push(Box::new(format!("%{kw}%")));
            }
            if let Some(after) = q.after {
                conditions.push("time >= ?");
                bind_values.push(Box::new(after.to_rfc3339()));
            }
            if let Some(before) = q.before {
                conditions.push("time <= ?");
                bind_values.push(Box::new(before.to_rfc3339()));
            }

            let where_clause = if conditions.is_empty() {
                String::new()
            } else {
                format!("WHERE {}", conditions.join(" AND "))
            };

            // Count total.
            let count_sql = format!("SELECT COUNT(*) FROM messages {where_clause}");
            let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                bind_values.iter().map(|b| b.as_ref()).collect();
            let total: u64 = lock
                .query_row(&count_sql, params_refs.as_slice(), |row| row.get(0))
                .map_err(|e| GatewayError::Storage(format!("count query failed: {e}")))?;

            // Fetch rows.
            let limit = q.limit.unwrap_or(50).min(500);
            let offset = q.offset.unwrap_or(0);
            let select_sql = format!(
                "SELECT id, time, self_id, message_type, message_id, user_id, group_id,
                        raw_message, sender_nickname, raw_json
                 FROM messages {where_clause}
                 ORDER BY time DESC
                 LIMIT {limit} OFFSET {offset}"
            );

            let params_refs2: Vec<&dyn rusqlite::types::ToSql> =
                bind_values.iter().map(|b| b.as_ref()).collect();
            let mut stmt = lock
                .prepare(&select_sql)
                .map_err(|e| GatewayError::Storage(format!("prepare failed: {e}")))?;

            let rows = stmt
                .query_map(params_refs2.as_slice(), |row| {
                    let time_str: String = row.get(1)?;
                    let time = NaiveDateTime::parse_from_str(&time_str, "%Y-%m-%dT%H:%M:%S%.f%:z")
                        .or_else(|_| {
                            NaiveDateTime::parse_from_str(&time_str, "%Y-%m-%dT%H:%M:%S%z")
                        })
                        .map(|naive| naive.and_utc())
                        .unwrap_or_else(|_| Utc::now());
                    Ok(StoredMessage {
                        id: row.get(0)?,
                        time,
                        self_id: row.get(2)?,
                        message_type: row.get(3)?,
                        message_id: row.get(4)?,
                        user_id: row.get(5)?,
                        group_id: row.get(6)?,
                        raw_message: row.get(7)?,
                        sender_nickname: row.get(8)?,
                        raw_json: row.get(9)?,
                    })
                })
                .map_err(|e| GatewayError::Storage(format!("query failed: {e}")))?;

            let mut messages = Vec::new();
            for row in rows {
                match row {
                    Ok(m) => messages.push(m),
                    Err(e) => warn!("skipping row: {e}"),
                }
            }

            Ok(MessageQueryResult { total, messages })
        })
        .await
        .map_err(|e| GatewayError::Storage(format!("spawn_blocking failed: {e}")))?
    }

    /// Delete messages older than `max_days`. Returns the number of rows deleted.
    pub async fn cleanup(&self) -> Result<u64, GatewayError> {
        let conn = Arc::clone(&self.conn);
        let max_days = self.max_days;

        tokio::task::spawn_blocking(move || {
            let lock = conn.lock();
            let deleted = lock
                .execute(
                    "DELETE FROM messages WHERE time < datetime('now', ?1)",
                    params![format!("-{max_days} days")],
                )
                .map_err(|e| GatewayError::Storage(format!("cleanup failed: {e}")))?;
            Ok(deleted as u64)
        })
        .await
        .map_err(|e| GatewayError::Storage(format!("spawn_blocking failed: {e}")))?
    }

    /// Get the total message count.
    pub async fn count(&self) -> Result<u64, GatewayError> {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let lock = conn.lock();
            let count: u64 = lock
                .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
                .map_err(|e| GatewayError::Storage(format!("count failed: {e}")))?;
            Ok(count)
        })
        .await
        .map_err(|e| GatewayError::Storage(format!("spawn_blocking failed: {e}")))?
    }
}

/// Internal helper: clone query params for moving into spawn_blocking.
impl MessageQuery {
    fn clone_for_blocking(&self) -> MessageQueryOwned {
        MessageQueryOwned {
            self_id: self.self_id,
            group_id: self.group_id,
            user_id: self.user_id,
            message_type: self.message_type.clone(),
            keyword: self.keyword.clone(),
            limit: self.limit,
            offset: self.offset,
            after: self.after,
            before: self.before,
        }
    }
}

/// Owned version of MessageQuery that is Send + 'static.
struct MessageQueryOwned {
    self_id: Option<i64>,
    group_id: Option<i64>,
    user_id: Option<i64>,
    message_type: Option<String>,
    keyword: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    after: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
}

/// Spawn a background cleanup task that runs every hour.
pub fn spawn_cleanup_task(store: Arc<MessageStore>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            match store.cleanup().await {
                Ok(deleted) => {
                    if deleted > 0 {
                        info!(deleted, "message store cleanup complete");
                    } else {
                        debug!("message store cleanup: nothing to delete");
                    }
                }
                Err(e) => {
                    error!("message store cleanup failed: {e}");
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ferroq_core::event::{MessageType, Sender};
    use uuid::Uuid;

    fn make_test_message(user_id: i64, group_id: Option<i64>, text: &str) -> MessageEvent {
        MessageEvent {
            id: Uuid::new_v4(),
            time: Utc::now(),
            self_id: 1234567890,
            message_type: if group_id.is_some() {
                MessageType::Group
            } else {
                MessageType::Private
            },
            sub_type: "normal".into(),
            message_id: rand::random::<i64>().abs(),
            user_id,
            group_id,
            message: vec![ferroq_core::message::MessageSegment::text(text)],
            raw_message: text.into(),
            sender: Sender {
                user_id,
                nickname: format!("User{user_id}"),
                card: None,
                sex: None,
                age: None,
                area: None,
                level: None,
                role: None,
                title: None,
            },
            font: 0,
        }
    }

    #[tokio::test]
    async fn insert_and_query() {
        let store = MessageStore::open_in_memory().unwrap();
        let msg = make_test_message(111, Some(999), "hello world");
        store.insert(&msg).await.unwrap();

        let result = store.query(&MessageQuery::default()).await.unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.messages[0].user_id, 111);
        assert_eq!(result.messages[0].group_id, Some(999));
        assert!(result.messages[0].raw_message.contains("hello"));
    }

    #[tokio::test]
    async fn query_with_filters() {
        let store = MessageStore::open_in_memory().unwrap();

        // Insert multiple messages.
        store
            .insert(&make_test_message(111, Some(999), "hello"))
            .await
            .unwrap();
        store
            .insert(&make_test_message(222, Some(999), "world"))
            .await
            .unwrap();
        store
            .insert(&make_test_message(111, None, "private msg"))
            .await
            .unwrap();

        // Filter by user_id.
        let result = store
            .query(&MessageQuery {
                user_id: Some(111),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(result.total, 2);

        // Filter by group_id.
        let result = store
            .query(&MessageQuery {
                group_id: Some(999),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(result.total, 2);

        // Filter by keyword.
        let result = store
            .query(&MessageQuery {
                keyword: Some("private".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(result.total, 1);
    }

    #[tokio::test]
    async fn count_messages() {
        let store = MessageStore::open_in_memory().unwrap();
        assert_eq!(store.count().await.unwrap(), 0);

        store
            .insert(&make_test_message(111, Some(999), "test"))
            .await
            .unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        store
            .insert(&make_test_message(222, None, "test2"))
            .await
            .unwrap();
        assert_eq!(store.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn pagination() {
        let store = MessageStore::open_in_memory().unwrap();

        for i in 0..10 {
            store
                .insert(&make_test_message(100 + i, Some(999), &format!("msg {i}")))
                .await
                .unwrap();
        }

        let result = store
            .query(&MessageQuery {
                limit: Some(3),
                offset: Some(0),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(result.total, 10);
        assert_eq!(result.messages.len(), 3);

        let result2 = store
            .query(&MessageQuery {
                limit: Some(3),
                offset: Some(3),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(result2.total, 10);
        assert_eq!(result2.messages.len(), 3);
    }
}
