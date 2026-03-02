//! Echo Plugin for ferroq.
//!
//! A simple plugin that repeats messages back to the sender.
//! Demonstrates the ferroq WASM plugin API.

use serde::{Deserialize, Serialize};
use std::alloc::{alloc, dealloc, Layout};
use std::cell::RefCell;

// ============================================================================
// Plugin Metadata
// ============================================================================

/// Plugin information.
#[derive(Debug, Clone, Serialize)]
struct PluginInfo {
    name: String,
    version: String,
    description: String,
    author: String,
}

// ============================================================================
// Event/Request Types (subset of ferroq types needed for this plugin)
// ============================================================================

/// Event type from ferroq.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "post_type")]
enum Event {
    #[serde(rename = "message")]
    Message(Box<MessageEvent>),
    #[serde(other)]
    Other,
}

/// Message event.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MessageEvent {
    #[serde(default)]
    self_id: i64,
    message_type: String,
    #[serde(default)]
    sub_type: String,
    #[serde(default)]
    message_id: i64,
    #[serde(default)]
    user_id: i64,
    #[serde(default)]
    group_id: Option<i64>,
    message: Vec<MessageSegment>,
    #[serde(default)]
    raw_message: String,
    #[serde(flatten)]
    extra: serde_json::Value,
}

/// Message segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
enum MessageSegment {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(other)]
    Other,
}

/// API request.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApiRequest {
    action: String,
    #[serde(default)]
    params: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    echo: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    self_id: Option<i64>,
}

// ============================================================================
// Configuration
// ============================================================================

/// Plugin configuration.
#[derive(Debug, Clone, Default, Deserialize)]
struct EchoConfig {
    /// Prefix to add before echoed text (default: "[echo] ").
    #[serde(default = "default_prefix")]
    prefix: String,

    /// Only echo messages containing this keyword (empty = all messages).
    #[serde(default)]
    keyword: String,

    /// Whether to echo in groups (default: true).
    #[serde(default = "default_true")]
    echo_group: bool,

    /// Whether to echo in private chats (default: true).
    #[serde(default = "default_true")]
    echo_private: bool,
}

fn default_prefix() -> String {
    "[echo] ".to_string()
}

fn default_true() -> bool {
    true
}

// ============================================================================
// Plugin State (thread-local for WASM single-threaded execution)
// ============================================================================

thread_local! {
    static CONFIG: RefCell<EchoConfig> = RefCell::new(EchoConfig::default());
    static RESULT_BUFFER: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}

// ============================================================================
// Host Function Imports (provided by ferroq)
// ============================================================================

unsafe extern "C" {
    /// Write result back to host.
    fn ferroq_set_result(ptr: *const u8, len: i32);

    /// Log a message (level: 0=trace, 1=debug, 2=info, 3=warn, 4=error).
    fn ferroq_log(level: i32, ptr: *const u8, len: i32);
}

/// Helper to call host's set_result function.
fn set_result(data: &[u8]) {
    unsafe {
        ferroq_set_result(data.as_ptr(), data.len() as i32);
    }
}

/// Helper to log messages.
fn log(level: i32, msg: &str) {
    let bytes = msg.as_bytes();
    unsafe {
        ferroq_log(level, bytes.as_ptr(), bytes.len() as i32);
    }
}

fn log_info(msg: &str) {
    log(2, msg);
}

fn log_debug(msg: &str) {
    log(1, msg);
}

// ============================================================================
// Memory Allocation (required by host)
// ============================================================================

/// Allocate memory for the host to write data into.
#[unsafe(no_mangle)]
pub extern "C" fn ferroq_alloc(size: i32) -> *mut u8 {
    if size <= 0 {
        return std::ptr::null_mut();
    }
    let layout = Layout::from_size_align(size as usize, 1).expect("layout");
    unsafe { alloc(layout) }
}

/// Free previously allocated memory.
#[unsafe(no_mangle)]
pub extern "C" fn ferroq_dealloc(ptr: *mut u8, size: i32) {
    if ptr.is_null() || size <= 0 {
        return;
    }
    let layout = Layout::from_size_align(size as usize, 1).expect("layout");
    unsafe { dealloc(ptr, layout) }
}

// ============================================================================
// Plugin Exports
// ============================================================================

/// Return plugin info as JSON.
#[unsafe(no_mangle)]
pub extern "C" fn ferroq_plugin_info() -> i32 {
    let info = PluginInfo {
        name: "echo".to_string(),
        version: "0.1.0".to_string(),
        description: "Echo plugin - repeats messages back".to_string(),
        author: "ferroq".to_string(),
    };

    match serde_json::to_vec(&info) {
        Ok(json) => {
            set_result(&json);
            0
        }
        Err(_) => -1,
    }
}

/// Initialize plugin with configuration.
#[unsafe(no_mangle)]
pub extern "C" fn ferroq_plugin_init(config_ptr: *const u8, config_len: i32) -> i32 {
    if config_ptr.is_null() || config_len <= 0 {
        log_info("echo plugin initialized with default config");
        return 0;
    }

    let config_bytes =
        unsafe { std::slice::from_raw_parts(config_ptr, config_len as usize) };

    match serde_json::from_slice::<EchoConfig>(config_bytes) {
        Ok(cfg) => {
            log_info(&format!(
                "echo plugin initialized: prefix='{}', keyword='{}'",
                cfg.prefix, cfg.keyword
            ));
            CONFIG.with(|c| *c.borrow_mut() = cfg);
            0
        }
        Err(e) => {
            log(4, &format!("failed to parse config: {}", e));
            -1
        }
    }
}

/// Process an incoming event.
/// Returns: 0=Continue, 1=Handled, 2=Drop, -1=Error
#[unsafe(no_mangle)]
pub extern "C" fn ferroq_on_event(event_ptr: *const u8, event_len: i32) -> i32 {
    if event_ptr.is_null() || event_len <= 0 {
        return 0; // Continue
    }

    let event_bytes =
        unsafe { std::slice::from_raw_parts(event_ptr, event_len as usize) };

    // Parse event
    let event: Event = match serde_json::from_slice(event_bytes) {
        Ok(e) => e,
        Err(e) => {
            log_debug(&format!("failed to parse event: {}", e));
            return 0; // Continue on parse error
        }
    };

    // Only handle message events
    let Event::Message(msg) = event else {
        return 0; // Continue for non-message events
    };

    // Get config
    let should_echo = CONFIG.with(|c| {
        let cfg = c.borrow();

        // Check message type filter
        if msg.message_type == "group" && !cfg.echo_group {
            return false;
        }
        if msg.message_type == "private" && !cfg.echo_private {
            return false;
        }

        // Check keyword filter
        if !cfg.keyword.is_empty() {
            let has_keyword = msg.message.iter().any(|seg| {
                if let MessageSegment::Text { text } = seg {
                    text.contains(&cfg.keyword)
                } else {
                    false
                }
            });
            if !has_keyword {
                return false;
            }
        }

        true
    });

    if !should_echo {
        return 0; // Continue
    }

    // Modify the message to add prefix
    let modified_msg = CONFIG.with(|c| {
        let cfg = c.borrow();
        let mut modified = (*msg).clone();

        // Add prefix to the first text segment
        for seg in &mut modified.message {
            if let MessageSegment::Text { text } = seg {
                *text = format!("{}{}", cfg.prefix, text);
                break;
            }
        }

        // Also update raw_message
        modified.raw_message = format!("{}{}", cfg.prefix, modified.raw_message);

        modified
    });

    // Serialize and set result
    let modified_event = Event::Message(Box::new(modified_msg));
    match serde_json::to_vec(&modified_event) {
        Ok(json) => {
            set_result(&json);
            log_debug("echo plugin modified message");
            0 // Continue with modified event
        }
        Err(e) => {
            log(4, &format!("failed to serialize modified event: {}", e));
            0 // Continue with original
        }
    }
}

/// Process an API call.
/// Returns: 0=Continue, 1=Handled, 2=Drop, -1=Error
#[unsafe(no_mangle)]
pub extern "C" fn ferroq_on_api_call(req_ptr: *const u8, req_len: i32) -> i32 {
    // This plugin doesn't need to modify API calls
    let _ = (req_ptr, req_len);
    0 // Continue
}
