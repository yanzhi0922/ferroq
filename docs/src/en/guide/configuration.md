# Configuration Reference

ferroq uses a YAML configuration file (`config.yaml` by default). This page documents every option.

## Server

```yaml
server:
  host: "0.0.0.0"       # Listen address
  port: 8080             # Listen port
  access_token: ""       # Global auth token (empty = no auth)
  dashboard: true        # Enable embedded web dashboard
  rate_limit:
    enabled: false       # Enable global rate limiting
    requests_per_second: 100
    burst: 200           # Token bucket burst size
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `host` | string | `"0.0.0.0"` | Bind address |
| `port` | u16 | `8080` | Bind port |
| `access_token` | string | `""` | Global Bearer token for API auth. Empty disables auth. |
| `dashboard` | bool | `true` | Serve the embedded web dashboard at `/dashboard/` |
| `rate_limit.enabled` | bool | `false` | Enable token-bucket rate limiting |
| `rate_limit.requests_per_second` | u64 | `100` | Refill rate |
| `rate_limit.burst` | u64 | `200` | Maximum burst size |

## Accounts

Each account maps to one QQ protocol backend.

```yaml
accounts:
  - name: "main"
    backend:
      type: lagrange             # lagrange, napcat, official, mock
      url: "ws://127.0.0.1:8081/onebot/v11/ws"
      access_token: ""
      reconnect_interval: 5      # seconds (base for exponential backoff)
      max_reconnect_interval: 120
      health_check_interval: 30
      connect_timeout: 15
      api_timeout: 30
    # Optional failover backend:
    # fallback:
    #   type: napcat
    #   url: "ws://127.0.0.1:3001"
    #   ...same fields as backend...
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `name` | string | required | Unique name for this account |
| `backend.type` | string | required | Backend type: `lagrange`, `napcat`, `official`, `mock` |
| `backend.url` | string | required | WebSocket URL of the backend |
| `backend.access_token` | string | `""` | Token to authenticate with the backend |
| `backend.reconnect_interval` | u64 | `5` | Base reconnect interval (seconds) |
| `backend.max_reconnect_interval` | u64 | `120` | Max reconnect interval after exponential backoff |
| `backend.health_check_interval` | u64 | `30` | Health check ping interval (seconds) |
| `backend.connect_timeout` | u64 | `15` | WebSocket connect timeout (seconds) |
| `backend.api_timeout` | u64 | `30` | API call response timeout (seconds) |
| `fallback` | object | none | Optional fallback backend for failover |

## Protocols

Configure how upstream bot frameworks connect to ferroq.

### OneBot v11

```yaml
protocols:
  onebot_v11:
    enabled: true
    http: true          # POST /onebot/v11/api/:action
    ws: true            # ws://host:port/onebot/v11/ws
    ws_reverse: []      # ferroq connects to these (reverse WS)
    http_post: []       # ferroq POSTs events to these
```

**Reverse WebSocket** targets:
```yaml
ws_reverse:
  - url: "ws://127.0.0.1:8765/onebot/v11/ws"
    access_token: ""
```

**HTTP POST** targets (events pushed via POST):
```yaml
http_post:
  - url: "http://127.0.0.1:5700"
    secret: ""          # HMAC-SHA1 signing secret
```

### OneBot v12

```yaml
protocols:
  onebot_v12:
    enabled: false
    http: true          # POST /onebot/v12/action
    ws: true            # ws://host:port/onebot/v12/ws
```

### Satori

```yaml
protocols:
  satori:
    enabled: false
    http: true          # POST /satori/v1/{resource}.{method}
    ws: true            # ws://host:port/satori/v1/events
```

## Storage

```yaml
storage:
  enabled: false
  path: "./data/messages.db"   # SQLite database path
  max_days: 30                 # Auto-cleanup after N days
```

## Deduplication

```yaml
dedup:
  enabled: true
  window_secs: 60     # Fingerprint time window
```

When failover is active, both primary and fallback may send the same event. The dedup filter drops duplicates using a time-windowed fingerprint map.

## Plugins

```yaml
plugins:
  - path: "./plugins/echo.wasm"
    enabled: true
    config:              # Arbitrary JSON passed to plugin_init()
      prefix: "[echo] "
      keyword: ""
```

See [Plugin System Overview](../plugins/overview.md) for details.

## Logging

```yaml
logging:
  level: "info"          # trace, debug, info, warn, error
  # file: "./logs/ferroq.log"
  console: true
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `FERROQ_CONFIG` | Path to config file (default: `config.yaml`) |
| `RUST_LOG` | Override log filter (standard `tracing` format) |
