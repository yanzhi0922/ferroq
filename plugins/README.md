# ferroq Plugins

This directory contains example WASM plugins for ferroq.

## Building Plugins

Plugins are compiled to WebAssembly (WASM). To build a plugin:

1. Install the WASM target:
   ```bash
   rustup target add wasm32-unknown-unknown
   ```

2. Build the plugin:
   ```bash
   cd plugins/echo
   cargo build --release --target wasm32-unknown-unknown
   ```

3. The compiled plugin will be at:
   ```
   target/wasm32-unknown-unknown/release/ferroq_plugin_echo.wasm
   ```

4. Configure ferroq to load the plugin:
   ```yaml
   plugins:
     - path: "./plugins/echo.wasm"
       enabled: true
       config:
         prefix: "[echo] "
         keyword: ""      # empty = echo all messages
         echo_group: true
         echo_private: true
   ```

## Plugin API

### Required Exports

Every plugin must export:

- `ferroq_plugin_info() -> i32`: Return plugin metadata as JSON via `ferroq_set_result`
- `ferroq_alloc(size: i32) -> *mut u8`: Allocate memory for host
- `ferroq_dealloc(ptr: *mut u8, size: i32)`: Free allocated memory

### Optional Exports

- `ferroq_plugin_init(config_ptr: *const u8, config_len: i32) -> i32`: Initialize with config
- `ferroq_on_event(event_ptr: *const u8, event_len: i32) -> i32`: Process events
- `ferroq_on_api_call(req_ptr: *const u8, req_len: i32) -> i32`: Process API calls

### Return Codes

- `0`: Continue (pass to next plugin / handler)
- `1`: Handled (stop processing)
- `2`: Drop (discard event/request)
- `-1`: Error

### Host Functions

Plugins can call these functions provided by ferroq:

- `ferroq_set_result(ptr: *const u8, len: i32)`: Write result data back to host
- `ferroq_log(level: i32, ptr: *const u8, len: i32)`: Log a message
  - Level: 0=trace, 1=debug, 2=info, 3=warn, 4=error

## Example Plugins

### echo

A simple plugin that adds a prefix to incoming messages. Useful as a template for new plugins.

Configuration:
```yaml
config:
  prefix: "[echo] "    # Prefix to add
  keyword: ""          # Only echo if message contains this
  echo_group: true     # Echo in group chats
  echo_private: true   # Echo in private chats
```
