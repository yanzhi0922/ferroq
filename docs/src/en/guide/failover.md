# Failover & Deduplication

ferroq supports automatic failover between a primary and fallback backend per account, with built-in event deduplication to prevent duplicate messages.

## Failover

### Configuration

```yaml
accounts:
  - name: "main"
    backend:
      type: lagrange
      url: "ws://127.0.0.1:8081/onebot/v11/ws"
    fallback:
      type: napcat
      url: "ws://127.0.0.1:3001"
```

### Behavior

When a fallback is configured, ferroq wraps the primary + fallback in a **FailoverAdapter**:

- **API calls** try the primary first. On connection error, the call is automatically retried on the fallback.
- **Events** flow from **both** backends simultaneously. This ensures no events are lost even if one backend disconnects.
- **Health checks** succeed if *either* backend is healthy.
- Both backends use independent exponential backoff reconnection.

### Reconnection

Each backend adapter uses exponential backoff:

1. Disconnect detected
2. Wait `reconnect_interval` seconds (default: 5)
3. Attempt reconnect
4. On failure: double the interval (capped at `max_reconnect_interval`)
5. On success: reset interval to base

## Event Deduplication

When failover is active, the same event may arrive from both primary and fallback backends.

### How It Works

```yaml
dedup:
  enabled: true
  window_secs: 60
```

The dedup filter maintains a time-windowed set of event **fingerprints**:

| Event Type | Fingerprint |
|------------|-------------|
| Message | `(self_id, message_id)` |
| Notice | `(self_id, notice_type, sub_type, time_sec, extra_hash)` |
| Request | `(self_id, request_type, sub_type, time_sec, extra_hash)` |
| Meta | `(self_id, meta_event_type, time_sec)` |

Events with the same fingerprint within `window_secs` are silently dropped.

### Performance

The dedup filter is O(1) per event — it uses a 128-bit hash key in a `HashMap` with lazy eviction. Benchmarks show:

- ~670 ns per unique event (cache miss)
- ~420 ns per duplicate event (cache hit)
- No degradation with 10,000+ cached fingerprints

### Monitoring

The `/health` endpoint reports `events_deduplicated` — the total number of duplicate events suppressed since startup.
