# Writing Your First Plugin

This guide walks you through creating a simple ferroq WASM plugin in Rust.

## Prerequisites

- Rust 1.85+ with the WASM target:
  ```bash
  rustup target add wasm32-unknown-unknown
  ```

## Step 1: Create the Project

```bash
cargo new --lib my_plugin
cd my_plugin
```

Edit `Cargo.toml`:

```toml
[package]
name = "my-plugin"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Exclude from parent workspace if inside ferroq tree
[workspace]
```

## Step 2: Define the Plugin

In `src/lib.rs`:

```rust
use std::cell::RefCell;

// === Host functions provided by ferroq ===
unsafe extern "C" {
    fn ferroq_set_result(ptr: *const u8, len: i32);
    fn ferroq_log(level: i32, ptr: *const u8, len: i32);
}

// === Global state ===
thread_local! {
    static CONFIG: RefCell<serde_json::Value> = RefCell::new(serde_json::Value::Null);
}

// === Required: Plugin info ===
#[unsafe(no_mangle)]
pub extern "C" fn ferroq_plugin_info() -> i32 {
    let info = serde_json::json!({
        "name": "my-plugin",
        "version": "0.1.0",
        "description": "My first ferroq plugin",
        "author": "me"
    });
    let bytes = serde_json::to_vec(&info).unwrap();
    unsafe { ferroq_set_result(bytes.as_ptr(), bytes.len() as i32); }
    0
}

// === Required: Memory allocation ===
#[unsafe(no_mangle)]
pub extern "C" fn ferroq_alloc(size: i32) -> *mut u8 {
    let layout = std::alloc::Layout::from_size_align(size as usize, 1).unwrap();
    unsafe { std::alloc::alloc(layout) }
}

#[unsafe(no_mangle)]
pub extern "C" fn ferroq_dealloc(ptr: *mut u8, size: i32) {
    let layout = std::alloc::Layout::from_size_align(size as usize, 1).unwrap();
    unsafe { std::alloc::dealloc(ptr, layout); }
}

// === Optional: Initialize with config ===
#[unsafe(no_mangle)]
pub extern "C" fn ferroq_plugin_init(config_ptr: *const u8, config_len: i32) -> i32 {
    let slice = unsafe { std::slice::from_raw_parts(config_ptr, config_len as usize) };
    if let Ok(config) = serde_json::from_slice::<serde_json::Value>(slice) {
        CONFIG.with(|c| *c.borrow_mut() = config);
    }
    log(2, "my-plugin initialized!");
    0
}

// === Optional: Process events ===
#[unsafe(no_mangle)]
pub extern "C" fn ferroq_on_event(event_ptr: *const u8, event_len: i32) -> i32 {
    let slice = unsafe { std::slice::from_raw_parts(event_ptr, event_len as usize) };
    if let Ok(event) = serde_json::from_slice::<serde_json::Value>(slice) {
        let post_type = event.get("post_type").and_then(|v| v.as_str()).unwrap_or("");
        log(1, &format!("received event: post_type={post_type}"));
    }
    0 // Continue — pass to next plugin
}

fn log(level: i32, msg: &str) {
    unsafe { ferroq_log(level, msg.as_ptr(), msg.len() as i32); }
}
```

## Step 3: Build

```bash
cargo build --release --target wasm32-unknown-unknown
```

The output is at `target/wasm32-unknown-unknown/release/my_plugin.wasm`.

## Step 4: Configure

Copy the `.wasm` file and add it to your ferroq config:

```yaml
plugins:
  - path: "./plugins/my_plugin.wasm"
    enabled: true
    config: {}
```

## Step 5: Run

Start ferroq — you should see log messages from your plugin:

```
INFO my-plugin initialized!
DEBUG received event: post_type=message
```

## Return Codes

Your `ferroq_on_event` and `ferroq_on_api_call` functions return an `i32`:

| Code | Meaning | Behavior |
|------|---------|----------|
| `0` | Continue | Pass to next plugin / handler |
| `1` | Handled | Stop plugin chain, use result from `ferroq_set_result` |
| `2` | Drop | Discard the event or API call entirely |
| `-1` | Error | Log error, continue processing |

## Tips

- Use `ferroq_set_result()` to pass modified event/API data back to ferroq
- Use `ferroq_log()` for observable debugging (levels: 0=trace, 1=debug, 2=info, 3=warn, 4=error)
- Keep allocations minimal — WASM linear memory is limited
- Test locally with `cargo test` before compiling to WASM
