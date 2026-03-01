//! # ferroq-gateway
//!
//! Core gateway logic for ferroq — the high-performance QQ Bot unified gateway.
//!
//! This crate contains:
//! - **Backend adapters** — connect to downstream QQ protocol backends
//! - **Protocol servers** — serve upstream bot frameworks
//! - **Event bus** — broadcast events from backends to protocol servers
//! - **Router** — route API calls from protocol servers to the correct backend
//! - **Storage** — optional message persistence (SQLite)

pub mod bus;
pub mod router;
pub mod runtime;
