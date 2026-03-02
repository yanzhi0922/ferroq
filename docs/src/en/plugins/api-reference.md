# Plugin API Reference

This page documents all functions that a plugin must/can export, and all host functions available to plugins.

## Required Exports

### `ferroq_plugin_info() -> i32`

Returns plugin metadata. Must call `ferroq_set_result()` with a JSON object:

```json
{
  "name": "plugin-name",
  "version": "0.1.0",
  "description": "What it does",
  "author": "Your Name"
}
```

**Returns:** `0` on success, `-1` on error.

### `ferroq_alloc(size: i32) -> *mut u8`

Allocates `size` bytes of memory in the plugin's linear memory. The host uses this to write event/request data before calling `ferroq_on_event` / `ferroq_on_api_call`.

**Implementation:**
```rust
#[unsafe(no_mangle)]
pub extern "C" fn ferroq_alloc(size: i32) -> *mut u8 {
    let layout = std::alloc::Layout::from_size_align(size as usize, 1).unwrap();
    unsafe { std::alloc::alloc(layout) }
}
```

### `ferroq_dealloc(ptr: *mut u8, size: i32)`

Frees memory previously allocated by `ferroq_alloc`. Called by the host after reading results.

## Optional Exports

### `ferroq_plugin_init(config_ptr: *const u8, config_len: i32) -> i32`

Called once after loading. `config_ptr` points to a UTF-8 JSON string containing the plugin's `config` from `config.yaml`.

**Returns:** `0` on success, `-1` on error.

### `ferroq_on_event(event_ptr: *const u8, event_len: i32) -> i32`

Called for each event flowing through the gateway. `event_ptr` points to a UTF-8 JSON string containing the full internal event.

**Event JSON format:**
```json
{
  "post_type": "message",
  "self_id": 123456789,
  "time": "2024-01-01T00:00:00Z",
  "message_type": "group",
  "message_id": 42,
  "user_id": 10001,
  "group_id": 900001,
  "message": [{"type": "text", "data": {"text": "hello"}}],
  "raw_message": "hello",
  "sender": {"user_id": 10001, "nickname": "Alice"}
}
```

**Returns:** Result code (see below).

### `ferroq_on_api_call(req_ptr: *const u8, req_len: i32) -> i32`

Called for each API request routed through the gateway. `req_ptr` points to a UTF-8 JSON string.

**Request JSON format:**
```json
{
  "action": "send_group_msg",
  "params": {
    "group_id": 900001,
    "message": "Hello!"
  },
  "self_id": 123456789
}
```

**Returns:** Result code (see below).

## Return Codes

| Code | Name | Behavior |
|------|------|----------|
| `0` | **Continue** | Pass event/request to the next plugin in the chain |
| `1` | **Handled** | Stop the plugin chain. If `ferroq_set_result()` was called, use that data as the modified event/response |
| `2` | **Drop** | Discard the event/request entirely — it will not reach any further plugins or the event bus |
| `-1` | **Error** | Log an error and continue as if `Continue` was returned |

## Host Functions

These functions are provided by ferroq and can be called from within plugin code.

### `ferroq_set_result(ptr: *const u8, len: i32)`

Write result data back to the host. Used to:
- Return plugin metadata from `ferroq_plugin_info()`
- Return a modified event from `ferroq_on_event()` (when returning `Handled`)
- Return a modified API request from `ferroq_on_api_call()` (when returning `Handled`)

The data must be a valid UTF-8 JSON string.

### `ferroq_log(level: i32, ptr: *const u8, len: i32)`

Log a message through ferroq's logging system.

**Levels:**
| Value | Level |
|-------|-------|
| `0` | trace |
| `1` | debug |
| `2` | info |
| `3` | warn |
| `4` | error |

## Memory Model

```
Host (ferroq)                    Plugin (WASM)
─────────────                    ─────────────
1. Call ferroq_alloc(N)  ───►   Allocate N bytes → ptr
2. Write data to ptr     ───►   (host writes to linear memory)
3. Call on_event(ptr, N) ───►   Process event
                         ◄───   Call ferroq_set_result(out_ptr, out_len)
4. Read result from WASM ◄───   (host reads from linear memory)
5. Call ferroq_dealloc() ───►   Free memory
```

The host always manages the lifecycle: allocate → write → call → read → deallocate.
