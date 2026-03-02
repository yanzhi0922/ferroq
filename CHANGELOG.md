# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.0-rc] - 2026-03-03

### Added
- **Official HTTP Adapter** — new `official` backend adapter for HTTP action APIs with route-mode auto-discovery.
- **Official Adapter Benchmarks** — criterion benchmark suite `official_adapter`.
- **Runtime Tuning Knobs** — env-configurable WS queue and in-flight limits:
  - `FERROQ_WS_OUTBOUND_QUEUE_CAPACITY`
  - `FERROQ_WS_API_MAX_IN_FLIGHT`
- **Competitive Ops Tooling**:
  - `scripts/competitive_snapshot.ps1`
  - `scripts/compare_gateways.ps1`
  - weekly snapshot workflow
  - growth execution plan

### Changed
- **WS Reliability Path** — bounded outbound channels + overload rejection metrics and handling across OneBot v11/v12/Satori.
- **Dashboard Routing UX** — `/dashboard` + `/dashboard/` compatibility, and dashboard can now be disabled by config.
- **Parser/Dedup Hot Path** — reduced allocations and lock contention in event parse/dedup flow.
- **Documentation** — refreshed EN/ZH docs for competitive benchmarking, runtime tuning, and official backend URL semantics.

### Quality Gates
- `cargo fmt --all`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `cargo bench --bench event_parse --bench dedup_filter --bench official_adapter -p ferroq-gateway`

## [0.1.0] - 2026-03-02

### Added
- CONTRIBUTING.md with contribution guidelines
- GitHub issue templates (bug report, feature request)
- CHANGELOG.md

- **Core Gateway** — event-driven architecture with `BackendAdapter` / `ProtocolServer` trait system
- **Lagrange.OneBot V2 Adapter** — WebSocket client with auto-reconnect (exponential backoff 1s→60s)
- **NapCat Adapter** — compatible via shared OneBot WS protocol, with field-level differences handled
- **OneBot v11 Protocol Server** — full implementation:
  - HTTP API (`POST /send_msg`, `GET /get_login_info`, etc.)
  - Forward WebSocket (`/ws`)
  - Reverse WebSocket client
  - HTTP POST event reporting
- **Multi-account Management** — multiple accounts via unified EventBus, dynamic add/remove at runtime
- **Failover** — automatic primary→fallback switching with health-check-based recovery
- **Message Persistence** — SQLite storage with `get_msg`, `get_group_msg_history`, auto-cleanup
- **Web Dashboard** — embedded single-page app: adapter status, message throughput, live stats
- **Management REST API** — runtime adapter CRUD, config reload, statistics endpoints
- **Rate Limiting** — token-bucket global rate limiter with configurable RPS and burst
- **Message Deduplication** — LRU-based dedup to prevent duplicate event forwarding
- **Auth Middleware** — access token validation with dynamic hot-reload support
- **Config Validation** — startup checks with severity-based error/warning reporting
- **Prometheus Metrics** — `/metrics` endpoint for monitoring integration
- **Docker Support** — Dockerfile (multi-stage Alpine) + docker-compose.yml
- **CI/CD** — GitHub Actions release workflow (6 platforms + Docker)
- **Documentation** — English README + Chinese README (README_ZH.md)
- **CLI** — `--generate-config`, `--config`, `--log-level`, graceful shutdown

[Unreleased]: https://github.com/yanzhi0922/ferroq/compare/v1.0.0-rc...HEAD
[1.0.0-rc]: https://github.com/yanzhi0922/ferroq/releases/tag/v1.0.0-rc
[0.1.0]: https://github.com/yanzhi0922/ferroq/releases/tag/v0.1.0
