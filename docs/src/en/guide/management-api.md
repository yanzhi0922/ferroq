# Management API

ferroq exposes a REST API for runtime management under the `/api` prefix. All endpoints require authentication when `access_token` is configured.

## Authentication

Include the token as:
- **Header**: `Authorization: Bearer YOUR_TOKEN`
- **Query**: `?access_token=YOUR_TOKEN`

## Endpoints

### Health Check

```
GET /health
```

Returns gateway health status (no auth required).

**Response:**
```json
{
  "status": "ok",
  "version": "0.1.0",
  "uptime_secs": 3600,
  "events_total": 12345,
  "events_deduplicated": 42,
  "api_calls_total": 678,
  "ws_connections": 2,
  "ws_connections_total": 15,
  "messages_stored": 5000,
  "storage_enabled": true,
  "healthy_adapters": 1,
  "total_adapters": 1,
  "adapters": [
    {
      "name": "main",
      "backend_type": "lagrange",
      "url": "ws://127.0.0.1:8081/onebot/v11/ws",
      "state": "Connected",
      "self_id": 123456789,
      "healthy": true,
      "health_check_ms": 5,
      "events_total": 12345,
      "api_calls_total": 678
    }
  ]
}
```

### Prometheus Metrics

```
GET /metrics
```

Returns metrics in Prometheus text format.

### List Accounts

```
GET /api/accounts
```

Returns all registered backend adapters with current status.

### Add Adapter

```
POST /api/accounts/add
Content-Type: application/json

{
  "name": "backup",
  "backend": {
    "type": "napcat",
    "url": "ws://127.0.0.1:3001",
    "access_token": "",
    "reconnect_interval": 5,
    "max_reconnect_interval": 120,
    "connect_timeout": 15,
    "api_timeout": 30
  }
}
```

Dynamically adds a new backend adapter at runtime.

### Remove Adapter

```
POST /api/accounts/{name}/remove
```

Disconnects and removes the named adapter.

### Reconnect Adapter

```
POST /api/accounts/{name}/reconnect
```

Forces a reconnect on the named adapter.

### Runtime Statistics

```
GET /api/stats
```

Returns detailed runtime statistics including per-adapter counters.

### Query Messages

```
GET /api/messages?group_id=900001&limit=50&offset=0
```

Query stored messages. Requires `storage.enabled: true`.

**Parameters:**
| Param | Type | Description |
|-------|------|-------------|
| `group_id` | i64 | Filter by group |
| `user_id` | i64 | Filter by sender |
| `keyword` | string | Full-text search |
| `limit` | u32 | Max results (default: 50) |
| `offset` | u32 | Pagination offset |

### View Config

```
GET /api/config
```

Returns the current configuration with secrets redacted.

### Hot Reload

```
POST /api/reload
```

Reloads access token and rate-limit parameters from config file without restarting. Returns the new config.
