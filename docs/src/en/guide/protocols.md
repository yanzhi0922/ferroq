# Protocol Servers

ferroq supports multiple inbound protocol servers simultaneously. Each protocol server translates between the protocol's wire format and ferroq's internal event types.

## OneBot v11

The primary and most complete protocol. Compatible with NoneBot2, Koishi, Yunzai, and most QQ bot frameworks.

### Endpoints

| Endpoint | Type | Description |
|----------|------|-------------|
| `POST /onebot/v11/api/:action` | HTTP | Call a OneBot v11 action |
| `POST /onebot/v11/api` | HTTP | Legacy endpoint (action in body) |
| `WS /onebot/v11/ws` | WebSocket | Forward WebSocket (bidirectional) |

### HTTP API

Send actions via HTTP POST:

```bash
curl -X POST http://localhost:8080/onebot/v11/api/send_group_msg \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"group_id": 123456, "message": "Hello!"}'
```

### Forward WebSocket

Connect with a WebSocket client to receive events and send actions:

```
ws://localhost:8080/onebot/v11/ws?access_token=YOUR_TOKEN
```

Events are pushed as JSON messages. Actions are sent as JSON and responses are returned with matching `echo` fields.

### Reverse WebSocket

ferroq connects *to* your bot as a client:

```yaml
protocols:
  onebot_v11:
    ws_reverse:
      - url: "ws://127.0.0.1:8765/onebot/v11/ws"
        access_token: ""
```

Supports exponential backoff reconnection on disconnect.

### HTTP POST

ferroq pushes events to your endpoint:

```yaml
protocols:
  onebot_v11:
    http_post:
      - url: "http://127.0.0.1:5700"
        secret: "your-hmac-secret"
```

When `secret` is set, each request includes an `X-Signature` header with HMAC-SHA1 signature.

---

## OneBot v12

The newer OneBot standard with a cleaner wire format.

### Endpoints

| Endpoint | Type | Description |
|----------|------|-------------|
| `POST /onebot/v12/action` | HTTP | Send an action (action name in body) |
| `POST /onebot/v12/action/:action` | HTTP | Send a named action |
| `WS /onebot/v12/ws` | WebSocket | Bidirectional WebSocket |

### Differences from v11

- Actions use a unified `{"action": "...", "params": {...}}` format
- Events include `"impl"` and `"platform"` fields
- No `post_type` — uses `"type"` field instead
- Responses use `{"status": "ok", "retcode": 0, "data": {...}}` format

---

## Satori

The [Satori protocol](https://satori.chat) is a cross-platform bot protocol.

### Endpoints

| Endpoint | Type | Description |
|----------|------|-------------|
| `POST /satori/v1/{resource}.{method}` | HTTP | Call a Satori API method |
| `WS /satori/v1/events` | WebSocket | Event stream with IDENTIFY/READY handshake |

### WebSocket Handshake

1. Client connects to `ws://host:port/satori/v1/events`
2. Client sends IDENTIFY signal:
   ```json
   {"op": 3, "body": {"token": "YOUR_TOKEN"}}
   ```
3. Server responds with READY:
   ```json
   {"op": 4, "body": {"logins": [...]}}
   ```
4. Server pushes EVENT signals (`op: 0`) with Satori-format events
5. Client must send PING (`op: 1`) periodically; server responds with PONG (`op: 2`)

### Event Format

Satori events use an HTML-like message element encoding:

```json
{
  "op": 0,
  "body": {
    "id": 1,
    "type": "message-created",
    "timestamp": 1700000000000,
    "channel": {"id": "900001", "type": 0},
    "user": {"id": "10001", "name": "Alice"},
    "message": {"id": "42", "content": "Hello <at id=\"10002\"/> world"}
  }
}
```
