# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- CONTRIBUTING.md with contribution guidelines
- GitHub issue templates (bug report, feature request)
- CHANGELOG.md

## [0.1.0] - 2026-03-02

### Added
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

[Unreleased]: https://github.com/yanzhi0922/ferroq/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/yanzhi0922/ferroq/releases/tag/v0.1.0
