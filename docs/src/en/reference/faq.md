# FAQ

## General

### What is ferroq?

ferroq is a high-performance QQ Bot protocol gateway written in Rust. It sits between QQ protocol backends (like Lagrange.OneBot or NapCat) and bot frameworks (like NoneBot2, Koishi, Yunzai), providing unified event routing, multi-protocol support, and reliability features.

### Why not just connect my bot framework directly to Lagrange/NapCat?

ferroq adds value when you need:
- **Multiple backends** — connect to Lagrange and NapCat simultaneously
- **Failover** — automatic switch between primary and fallback
- **Multi-protocol** — serve OneBot v11, v12, and Satori from the same backend
- **Observability** — health checks, Prometheus metrics, web dashboard
- **Plugins** — custom message processing in WASM
- **Message storage** — SQLite-backed message persistence

If you only have one backend and one bot framework, a direct connection is simpler.

### What protocols does ferroq support?

**Inbound (to bot frameworks):**
- OneBot v11 (HTTP, WS, reverse WS, HTTP POST)
- OneBot v12 (HTTP, WS)
- Satori (HTTP, WS)

**Outbound (to backends):**
- Lagrange.OneBot (WebSocket)
- NapCat (WebSocket)
- Official adapter (HTTP action API)

### Does ferroq implement the QQ protocol?

No. ferroq is a **gateway/proxy**, not a protocol implementation. It relies on backends like Lagrange.OneBot or NapCat to handle the actual QQ protocol.

## Performance

### How fast is ferroq?

End-to-end event forwarding latency is ~17.5 µs (small events) / ~37.8 µs (1 KB events). Throughput exceeds 290K msg/s. Memory usage is under 5 MB idle.

See [Performance](./performance.md) for detailed benchmarks.

### How much memory does ferroq use?

- Idle: ~5 MB RSS
- After processing 100K events: ~10 MB
- With 1K × 1KB events buffered: ~13 MB

## Plugins

### What languages can I write plugins in?

Any language that compiles to WebAssembly (wasm32-unknown-unknown). Rust is the most natural choice, but C, C++, AssemblyScript, and others work too.

### Can plugins access the network or filesystem?

No. Plugins run in a sandboxed wasmtime environment with no WASI capabilities. They can only process data passed to them by ferroq and return results.

### Can I disable the plugin system?

Yes. Build without the `wasm-plugins` feature:
```bash
cargo build --release --no-default-features -p ferroq-gateway
```

## Operations

### How do I update the config without restarting?

Use the hot-reload endpoint:
```bash
curl -X POST http://localhost:8080/api/reload \
  -H "Authorization: Bearer YOUR_TOKEN"
```

This reloads `access_token` and `rate_limit` parameters.

### How do I add a backend at runtime?

Use the management API:
```bash
curl -X POST http://localhost:8080/api/accounts/add \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "backup", "backend": {"type": "napcat", "url": "ws://..."}}'
```

### How do I monitor ferroq?

- **Health check:** `GET /health` — JSON with uptime, counters, adapter status
- **Prometheus:** `GET /metrics` — standard Prometheus text format
- **Dashboard:** `GET /dashboard` (or `/dashboard/`) — embedded web UI
- **Logs:** structured logging via `tracing` (JSON format available)
