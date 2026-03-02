<div align="center">

# ⚡ ferroq

**High-performance QQ Bot unified gateway** — written in pure Rust

[![CI](https://github.com/yanzhi0922/ferroq/actions/workflows/ci.yml/badge.svg)](https://github.com/yanzhi0922/ferroq/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/yanzhi0922/ferroq)](https://github.com/yanzhi0922/ferroq/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Docker](https://img.shields.io/badge/docker-ghcr.io-blue)](https://ghcr.io/yanzhi0922/ferroq)

*One gateway to rule them all — connect any QQ protocol backend, serve any bot framework.*

**English** | [简体中文](README_ZH.md)

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
                          │  OneBot v11 / v12 / Satori
                          ▼
┌──────────────────────────────────────────────────────────┐
│                     ⚡ ferroq                             │
│  ┌──────────────┐  ┌──────────┐  ┌────────────────────┐ │
│  │ Protocol     │  │ Event    │  │ Backend            │ │
│  │ Servers      │◄─┤ Bus      │◄─┤ Adapters           │ │
│  │ (inbound)    │  │          │  │ (outbound)         │ │
│  │              │  │          │  │                    │ │
│  │ • OneBot v11 │  │ broadcast│  │ • Lagrange WS      │ │
│  │ • OneBot v12 │  │ + dedup  │  │ • NapCat WS        │ │
│  │ • Satori     │  │ + plugin │  │ • Official API     │ │
│  │              │  │          │  │                    │ │
│  └──────────────┘  └──────────┘  └────────────────────┘ │
│  ┌──────────┐  ┌────────────┐  ┌────────────────────┐   │
│  │Dashboard │  │ Management │  │ Message Storage    │   │
│  │ (Web UI) │  │ API (/api) │  │ (SQLite)           │   │
│  └──────────┘  └────────────┘  └────────────────────┘   │
│  ┌──────────────────────────────────────────────────┐    │
│  │         WASM Plugin Engine (wasmtime)            │    │
│  └──────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────┘
                          │
                          ▼
┌──────────────────────────────────────────────────────────┐
│                   QQ Protocol Backends                    │
│     Lagrange.OneBot  /  NapCat  /  Official Bot API       │
└──────────────────────────────────────────────────────────┘
```

## Quick Start

### Docker (Recommended)

```bash
# Download example config
curl -LO https://raw.githubusercontent.com/yanzhi0922/ferroq/main/config.example.yaml
mv config.example.yaml config.yaml
# Edit config.yaml with your backend URL

# Run
docker run -d \
  --name ferroq \
  -p 8080:8080 \
  -v $(pwd)/config.yaml:/app/config.yaml:ro \
  -v $(pwd)/data:/app/data \
  ghcr.io/yanzhi0922/ferroq:latest
```

### Pre-built Binaries

Download from [Releases](https://github.com/yanzhi0922/ferroq/releases):

```bash
# Linux x86_64
curl -LO https://github.com/yanzhi0922/ferroq/releases/latest/download/ferroq-linux-x86_64.tar.gz
tar xzf ferroq-linux-x86_64.tar.gz
chmod +x ferroq
./ferroq --generate-config
./ferroq
```

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
| `POST /api/accounts/add` | Add a new adapter at runtime |
| `POST /api/accounts/{name}/remove` | Remove an adapter |
| `POST /api/accounts/{name}/reconnect` | Reconnect a specific adapter |
| `GET /api/stats` | Full runtime statistics |
| `GET /api/messages` | Query stored messages (with filters, pagination) |
| `GET /api/config` | View current config (secrets redacted) |
| `POST /api/reload` | Hot-reload access token and rate-limit params |
| `POST /onebot/v11/api/:action` | OneBot v11 HTTP API |
| `WS /onebot/v11/ws` | OneBot v11 forward WebSocket |
| `POST /onebot/v12/api/:action` | OneBot v12 HTTP API |
| `WS /onebot/v12/ws` | OneBot v12 WebSocket |
| `POST /satori/v1/` | Satori HTTP API |
| `WS /satori/v1/events` | Satori event WebSocket |

## Performance

Measured with [criterion](https://github.com/bheisler/criterion.rs) on real code paths:

| Metric | Value |
|--------|-------|
| End-to-end pipeline latency (small event) | **17.5 µs** |
| End-to-end pipeline latency (1KB event) | **37.8 µs** |
| Event bus throughput | **2.16M msg/s** |
| Event parse (OneBot v11) | **2.6 µs** |
| Dedup filter check | **670 ns** |
| Memory (idle) | **4.9 MB** |
| Memory (100K events processed) | **10 MB** |

See [BENCHMARK.md](BENCHMARK.md) for full results, methodology, and how to reproduce.

## Roadmap

- [x] **Phase 1** — Core types, traits, config, error handling
- [x] **Phase 2** — Gateway infrastructure: adapters, event bus, router, storage, auth, rate limiting, dashboard, management API, failover, dedup
- [x] **Phase 3.1** — WASM plugin system (wasmtime sandbox)
- [x] **Phase 3.2** — Multi-protocol servers: OneBot v11, OneBot v12, Satori
- [x] **Phase 3.3** — Performance benchmark suite (criterion)
- [x] **Phase 3.4** — Bilingual documentation site (mdbook)
- [ ] **v1.0.0** — Production release

## Project Structure

```
ferroq/
├── crates/
│   ├── ferroq-core/       # Core types, traits, config, validation (no I/O)
│   ├── ferroq-gateway/    # Gateway: adapters, bus, router, stats, storage, management
│   │   ├── src/
│   │   │   ├── adapter/   # Backend adapters (Lagrange, ...)
│   │   │   ├── server/    # Protocol servers (OneBot v11/v12, Satori)
│   │   │   ├── bus.rs     # Event bus (broadcast)
│   │   │   ├── router.rs  # API request router (self_id → adapter)
│   │   │   ├── stats.rs   # Runtime stats + health + Prometheus
│   │   │   ├── storage.rs # SQLite message store
│   │   │   ├── management.rs  # REST management API
│   │   │   ├── middleware.rs   # Auth + rate limiting
│   │   │   ├── plugin_engine.rs # WASM plugin runtime (wasmtime)
│   │   │   └── runtime.rs # Gateway lifecycle orchestration
│   │   └── tests/         # Integration tests with mock backend
│   ├── ferroq-web/        # Embedded web dashboard
│   └── ferroq/            # CLI binary entry point
├── config.example.yaml    # Full configuration reference
├── docs/                  # mdbook documentation (en + zh)
├── BENCHMARK.md           # Performance benchmark results
├── .github/workflows/     # CI: check, clippy, test (Linux/Windows/macOS), fmt
└── Cargo.toml             # Workspace root
```

## Testing

```bash
# Run all tests (96 total)
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
