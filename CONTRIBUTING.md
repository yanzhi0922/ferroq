# Contributing to ferroq

Thank you for your interest in contributing to ferroq! This document provides guidelines and information for contributors.

## Getting Started

### Prerequisites

- **Rust 1.85+** (stable) — enforced by `rust-toolchain.toml`
- **Git**

### Setup

```bash
git clone https://github.com/yanzhi0922/ferroq.git
cd ferroq
cargo build
cargo test --workspace
```

### Project Structure

```
crates/
├── ferroq-core/       # Core types, traits, config
├── ferroq-gateway/    # Gateway logic: adapters, router, storage, event bus
├── ferroq-web/        # Web Dashboard (embedded HTML/JS)
└── ferroq/            # CLI binary entry point
```

## Development Workflow

### 1. Find or Create an Issue

- Check [existing issues](https://github.com/yanzhi0922/ferroq/issues)
- Look for issues labeled `good first issue` for easy starting points
- If your change is non-trivial, open an issue first to discuss the approach

### 2. Create a Branch

```bash
git checkout -b feat/your-feature   # for features
git checkout -b fix/your-bugfix     # for bug fixes
```

### 3. Write Code

Follow these conventions:

- **All `pub` items must have `///` doc comments**
- **No `unwrap()` / `expect()`** in non-test code
- **Error messages in English**
- **Logging via `tracing` macros** (`trace!`, `debug!`, `info!`, `warn!`, `error!`)
- **Each module should have `#[cfg(test)] mod tests`**
- **`unsafe` code requires `// SAFETY:` comments**

### 4. Run Checks

```bash
# Format check
cargo fmt --check

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# Tests
cargo test --workspace
```

All three must pass — CI enforces this.

### 5. Submit a Pull Request

- Write a clear title and description
- Reference related issues (e.g., `Closes #123`)
- Keep PRs focused — one feature or fix per PR
- Ensure CI passes

## Code Style

### Error Handling

```rust
// ✅ Good — use anyhow or custom error types
fn connect(&self) -> Result<(), GatewayError> { ... }

// ❌ Bad — panics in production code
fn connect(&self) { self.inner.lock().unwrap() }
```

### Trait-First Design

```rust
// ✅ Good — program to traits
async fn route(&self, req: ApiRequest) -> Result<ApiResponse, GatewayError>;

// ❌ Bad — tied to concrete type
async fn route(&self, req: ApiRequest, adapter: &LagrangeAdapter) -> ...;
```

### Logging

```rust
use tracing::{debug, info, warn, error};

info!(adapter = %name, self_id, "connected to backend");
warn!(err = %e, "reconnection failed, retrying in {delay}s");
```

## Architecture Overview

```
Bot Framework (NoneBot, Koishi, ...)
        │
        ▼
  ProtocolServer (OneBot v11, v12, Satori, ...)
        │
        ▼
  EventBus ←→ ApiRouter
        │
        ▼
  BackendAdapter (Lagrange, NapCat, Official API, ...)
        │
        ▼
  QQ Protocol Backend
```

- **BackendAdapter** — connects to a downstream QQ backend, normalizes events
- **EventBus** — broadcasts events from backends to all protocol servers
- **ApiRouter** — routes API calls to the correct backend by `self_id`
- **ProtocolServer** — exposes an inbound protocol to upstream bot frameworks

## Adding a New Backend Adapter

1. Implement `BackendAdapter` trait from `ferroq-core`
2. Add the adapter module in `ferroq-gateway/src/adapter/`
3. Register the new backend type in `main.rs` adapter creation logic
4. Add tests
5. Update `config.example.yaml` with example configuration

## Adding a New Protocol Server

1. Implement the protocol module in `ferroq-gateway/src/server/`
2. Add configuration types to `ferroq-core/src/config.rs`
3. Wire up in `main.rs`
4. Add tests
5. Update `config.example.yaml`

## Reporting Issues

- Use the [issue templates](https://github.com/yanzhi0922/ferroq/issues/new/choose)
- Include your OS, Rust version, and ferroq version
- For bugs, include steps to reproduce and relevant logs

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
