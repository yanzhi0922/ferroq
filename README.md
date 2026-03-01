<div align="center">

# ⚡ ferroq

**High-performance QQ Bot unified gateway** — written in pure Rust

[![CI](https://github.com/YanZhangN/ferroq/actions/workflows/ci.yml/badge.svg)](https://github.com/YanZhangN/ferroq/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

*One gateway to rule them all — connect any QQ protocol backend, serve any bot framework.*

</div>

---

## What is ferroq?

**ferroq** is a high-performance **QQ Bot protocol gateway** that sits between QQ protocol backends (Lagrange, NapCat, Official API) and bot frameworks (NoneBot2, Koishi, Yunzai, etc.).

Instead of reimplementing the QQ protocol, ferroq acts as a **unified proxy / router**, providing:

- 🚀 **Extreme performance** — async Rust, zero-copy message forwarding, <1ms added latency
- 🔄 **Multi-protocol support** — OneBot v11, OneBot v12, Milky, Satori (planned)
- 🔌 **Backend agnostic** — Lagrange.OneBot, NapCat, Official API — hot-swap without restarting
- 📊 **Built-in dashboard** — web UI for monitoring, message logs, and configuration
- 🛡️ **Reliability** — auto-reconnect, health checks, failover between backends
- 💾 **Message storage** — optional SQLite-based message persistence
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
│                     ┌──────────┐                         │
│                     │ Dashboard │ ← Web UI (embedded)    │
│                     └──────────┘                         │
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
git clone https://github.com/YanZhangN/ferroq.git
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

accounts:
  - name: "main"
    backend:
      type: lagrange
      url: "ws://127.0.0.1:8081/onebot/v11/ws"

protocols:
  onebot_v11:
    enabled: true
    http: true
    ws: true
```

See [config.example.yaml](config.example.yaml) for the full configuration reference.

## Performance

| Metric | ferroq | go-cqhttp | Overflow |
|--------|--------|-----------|----------|
| Event forwarding latency | <1ms | ~5ms | ~3ms |
| Memory usage (idle) | ~8MB | ~30MB | ~50MB |
| Binary size | ~15MB | ~25MB | ~40MB |
| Concurrent connections | 10,000+ | ~1,000 | ~500 |

*Benchmarks coming soon — numbers are design targets.*

## Roadmap

- [x] **Phase 1** — Core skeleton + Lagrange adapter + OneBot v11
- [ ] **Phase 2** — NapCat adapter + Dashboard + Storage + Multi-account
- [ ] **Phase 3** — Plugin system + Milky/Satori + Benchmarks + Release

## Project Structure

```
ferroq/
├── crates/
│   ├── ferroq-core/       # Core types, traits, error types (no I/O)
│   ├── ferroq-gateway/    # Gateway logic: adapters, event bus, router
│   ├── ferroq-web/        # Web dashboard (embedded)
│   └── ferroq/            # CLI binary entry point
├── config.example.yaml    # Example configuration
└── Cargo.toml             # Workspace root
```

## License

[MIT](LICENSE)

---

<div align="center">

**⚡ Built with Rust for maximum performance ⚡**

</div>
