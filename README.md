<div align="center">

# ⚡ ferroq

**High-performance QQ Bot unified gateway** — written in pure Rust

[![CI](https://github.com/yanzhi0922/ferroq/actions/workflows/ci.yml/badge.svg)](https://github.com/yanzhi0922/ferroq/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

*One gateway to rule them all — connect any QQ protocol backend, serve any bot framework.*

</div>

---

## What is ferroq?

**ferroq** is a high-performance **QQ Bot protocol gateway** that sits between QQ protocol backends (Lagrange, NapCat, Official API) and bot frameworks (NoneBot2, Koishi, Yunzai, etc.).

Instead of reimplementing the QQ protocol, ferroq acts as a **unified proxy / router**, providing:

- 🚀 **Extreme performance** — async Rust, zero-copy message forwarding, <1ms added latency
- 🔄 **Multi-protocol support** — OneBot v11 (full), with OneBot v12 / Milky / Satori planned
- 🔌 **Backend agnostic** — Lagrange.OneBot, NapCat — hot-swap without restarting
- 📊 **Built-in dashboard** — web UI for monitoring adapters, per-adapter event/API metrics
- 🛡️ **Reliability** — exponential backoff reconnect, health checks, configurable timeouts
- � **Failover** — automatic primary/fallback adapter switching on connection errors
- 🧹 **Event deduplication** — time-windowed fingerprint filter, prevents duplicates from failover
- �💾 **Message storage** — optional SQLite-based message persistence with search & pagination
- 🔒 **Security** — Bearer / query-param auth, HMAC-SHA1 HTTP POST signing, secret redaction
- ⚡ **Hot reload** — `POST /api/reload` updates access token and rate-limit params without restart
- 📈 **Observability** — Prometheus `/metrics`, per-adapter event/API counters, health API
- 🚦 **Rate limiting** — global token-bucket limiter with `Retry-After` header, hot-reloadable params
- 📦 **Single binary** — one `ferroq` binary, no runtime dependencies, <15MB

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│                      Bot Frameworks                       │
│        NoneBot2 / Koishi / Yunzai / Custom Bot            │
└─────────────────────────┬────────────────────────────────┘
                          │  OneBot v11 / v12 / Milky / Satori
                          ▼
┌──────────────────────────────────────────────────────────┐
│                     ⚡ ferroq                             │
│  ┌──────────────┐  ┌──────────┐  ┌────────────────────┐ │
│  │ Protocol     │  │ Event    │  │ Backend            │ │
│  │ Servers      │◄─┤ Bus      │◄─┤ Adapters           │ │
│  │ (inbound)    │  │          │  │ (outbound)         │ │
│  │              │  │          │  │                    │ │
│  │ • OneBot v11 │  │ broadcast│  │ • Lagrange WS      │ │
│  │ • OneBot v12 │  │ + route  │  │ • NapCat WS        │ │
│  │ • Milky      │  │          │  │ • Official API     │ │
│  │ • Satori     │  │          │  │                    │ │
│  └──────────────┘  └──────────┘  └────────────────────┘ │
│  ┌──────────┐  ┌────────────┐  ┌────────────────────┐   │
│  │Dashboard │  │ Management │  │ Message Storage    │   │
│  │ (Web UI) │  │ API (/api) │  │ (SQLite)           │   │
│  └──────────┘  └────────────┘  └────────────────────┘   │
└──────────────────────────────────────────────────────────┘
                          │
                          ▼
┌──────────────────────────────────────────────────────────┐
│                   QQ Protocol Backends                    │
│     Lagrange.OneBot  /  NapCat  /  Official Bot API       │
└──────────────────────────────────────────────────────────┘
```

## Quick Start

### From Source

```bash
git clone https://github.com/yanzhi0922/ferroq.git
cd ferroq
cargo build --release

# Generate default config
./target/release/ferroq --generate-config

# Edit config.yaml, then:
./target/release/ferroq
```

### Configuration

```yaml
server:
  host: "0.0.0.0"
  port: 8080
  access_token: "your-secret-token"
  dashboard: true
  rate_limit:
    enabled: true
    requests_per_second: 100
    burst: 200

accounts:
  - name: "main"
    backend:
      type: lagrange
      url: "ws://127.0.0.1:8081/onebot/v11/ws"
      reconnect_interval: 5
      max_reconnect_interval: 120
      connect_timeout: 15
      api_timeout: 30

protocols:
  onebot_v11:
    enabled: true
    http: true
    ws: true
    ws_reverse: []
    http_post: []

storage:
  enabled: false
  path: "./data/messages.db"
  max_days: 30
```

See [config.example.yaml](config.example.yaml) for the full configuration reference.

## Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /health` | Health check — JSON with uptime, counters, adapter snapshots |
| `GET /metrics` | Prometheus-format metrics (per-adapter event/API counters) |
| `GET /dashboard/` | Embedded web dashboard |
| `GET /api/accounts` | List all registered backend adapters |
| `GET /api/stats` | Full runtime statistics |
| `GET /api/messages` | Query stored messages (with filters, pagination) |
| `GET /api/config` | View current config (secrets redacted) |
| `POST /api/reload` | Hot-reload access token and rate-limit params |
| `POST /onebot/v11/api/:action` | OneBot v11 HTTP API |
| `WS /onebot/v11/ws` | OneBot v11 forward WebSocket |

## Performance

| Metric | ferroq | go-cqhttp | Overflow |
|--------|--------|-----------|----------|
| Event forwarding latency | <1ms | ~5ms | ~3ms |
| Memory usage (idle) | ~8MB | ~30MB | ~50MB |
| Binary size | ~15MB | ~25MB | ~40MB |
| Concurrent connections | 10,000+ | ~1,000 | ~500 |

*Benchmarks coming soon — numbers are design targets.*

## Roadmap

- [x] **Phase 1** — Core skeleton: traits, config, error types
- [x] **Phase 2** — Lagrange adapter + event bus + OneBot v11 HTTP/WS server
- [x] **Phase 3.1** — Integration tests + mock backend + forward WS pipeline
- [x] **Phase 3.2** — Config validation + web dashboard + health API
- [x] **Phase 3.3** — SQLite message storage + management REST API
- [x] **Phase 3.4** — Auth middleware + rate limiting + hot-reload + Prometheus
- [x] **Phase 3.5** — Exponential backoff reconnect + per-adapter counters + configurable timeouts
- [x] **Phase 3.6** — Per-adapter API metrics + route_named + dashboard columns + Retry-After
- [x] **Phase 3.7** — Management API tests + testing hardening + documentation
- [x] **Phase 3.8** — Failover adapters + adapter type accuracy
- [x] **Phase 3.9** — Event deduplication + reverse WS exponential backoff
- [ ] **Phase 4** — Multi-account routing + failover + NapCat adapter
- [ ] **Phase 5** — OneBot v12 / Milky / Satori protocol servers
- [ ] **Phase 6** — Plugin system + benchmarks + release

## Project Structure

```
ferroq/
├── crates/
│   ├── ferroq-core/       # Core types, traits, config, validation (no I/O)
│   ├── ferroq-gateway/    # Gateway: adapters, bus, router, stats, storage, management
│   │   ├── src/
│   │   │   ├── adapter/   # Backend adapters (Lagrange, ...)
│   │   │   ├── server/    # Protocol servers (OneBot v11, ...)
│   │   │   ├── bus.rs     # Event bus (broadcast)
│   │   │   ├── router.rs  # API request router (self_id → adapter)
│   │   │   ├── stats.rs   # Runtime stats + health + Prometheus
│   │   │   ├── storage.rs # SQLite message store
│   │   │   ├── management.rs  # REST management API
│   │   │   ├── middleware.rs   # Auth + rate limiting
│   │   │   └── runtime.rs # Gateway lifecycle orchestration
│   │   └── tests/         # Integration tests with mock backend
│   ├── ferroq-web/        # Embedded web dashboard
│   └── ferroq/            # CLI binary entry point
├── config.example.yaml    # Full configuration reference
├── .github/workflows/     # CI: check, clippy, test (Linux/Windows/macOS), fmt
└── Cargo.toml             # Workspace root
```

## Testing

```bash
# Run all tests (73 total: 16 core + 52 gateway + 5 integration)
cargo test --workspace

# Run with clippy
cargo clippy --workspace -- -D warnings
```

## License

[MIT](LICENSE)

---

<div align="center">

**⚡ Built with Rust for maximum performance ⚡**

</div>
