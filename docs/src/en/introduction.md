# ferroq

> **High-performance QQ Bot unified gateway** — written in pure Rust

[![CI](https://github.com/yanzhi0922/ferroq/actions/workflows/ci.yml/badge.svg)](https://github.com/yanzhi0922/ferroq/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/yanzhi0922/ferroq)](https://github.com/yanzhi0922/ferroq/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://github.com/yanzhi0922/ferroq/blob/main/LICENSE)

**ferroq** is a high-performance QQ Bot protocol gateway that sits between QQ protocol backends (Lagrange, NapCat, Official API) and bot frameworks (NoneBot2, Koishi, Yunzai, etc.).

Instead of reimplementing the QQ protocol, ferroq acts as a **unified proxy / router**, providing:

- ⚡ **Extreme performance** — async Rust, <20µs end-to-end forwarding latency, 290K+ msg/s throughput
- 🔄 **Multi-protocol** — OneBot v11 (full), OneBot v12, Satori
- 🔌 **Backend agnostic** — Lagrange.OneBot, NapCat — hot-swap without restarting
- 🧩 **WASM plugins** — extend with custom logic in sandboxed WebAssembly
- 🛡️ **Reliable** — exponential backoff reconnect, failover, event deduplication
- 💾 **Message storage** — optional SQLite persistence with search & pagination
- 🔒 **Secure** — Bearer auth, HMAC-SHA1 signing, token hot-reload
- 📦 **Single binary** — one `ferroq` binary, no runtime dependencies, <15MB

## Who is ferroq for?

- **Bot developers** who want a single gateway to abstract over multiple backends
- **Platform operators** running multiple QQ accounts that need centralized event routing
- **Plugin authors** who want to write message-processing logic in Rust/WASM
- Anyone tired of modifying their bot when switching from Lagrange to NapCat or vice-versa

## Language

This documentation is available in:
- **English** (you are here)
- [简体中文](https://github.com/yanzhi0922/ferroq/tree/main/docs/src/zh)
