# Plugin System Overview

ferroq supports WASM-based plugins that can intercept and modify events and API calls as they flow through the gateway.

## Architecture

```
Backend → [parse] → [dedup] → [plugin chain] → [event bus] → Protocol Servers
                                     ↑
                              Plugin 1 → Plugin 2 → ...
```

Plugins are loaded from `.wasm` files and executed in a sandboxed [wasmtime](https://wasmtime.dev/) runtime. Each plugin can:

- **Inspect events** — log, filter, or transform message events
- **Inspect API calls** — modify or block outgoing API requests
- **Return results** — indicate whether to continue, handle, or drop

## Key Properties

- **Sandboxed** — plugins run in WebAssembly, isolated from the host process
- **Zero-trust** — plugins cannot access the filesystem, network, or host memory
- **Language agnostic** — any language that compiles to WASM works (Rust, C, AssemblyScript, etc.)
- **Hot-manageable** — enable/disable plugins via the management API
- **Fast** — wasmtime JIT-compiles WASM to native code

## Configuration

```yaml
plugins:
  - path: "./plugins/echo.wasm"
    enabled: true
    config:                # Passed as JSON to plugin_init()
      prefix: "[echo] "
      keyword: ""
```

## Plugin Lifecycle

1. **Load** — ferroq reads the `.wasm` file and instantiates it via wasmtime
2. **Info** — ferroq calls `ferroq_plugin_info()` to get plugin name, version, author
3. **Init** — ferroq calls `ferroq_plugin_init(config_json)` with the plugin's config
4. **Event processing** — for each event, ferroq calls `ferroq_on_event(event_json)`
5. **API processing** — for each API call, ferroq calls `ferroq_on_api_call(request_json)`

## Feature Gate

The WASM plugin system is behind the `wasm-plugins` cargo feature (enabled by default). To build without it:

```bash
cargo build --release --no-default-features -p ferroq-gateway
```

This removes the wasmtime dependency, reducing binary size significantly.
