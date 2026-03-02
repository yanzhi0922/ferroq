//! # ferroq-gateway
//!
//! Core gateway logic for ferroq — the high-performance QQ Bot unified gateway.
//!
//! This crate contains:
//! - **Backend adapters** — connect to downstream QQ protocol backends
//! - **Protocol servers** — serve upstream bot frameworks
//! - **Event bus** — broadcast events from backends to protocol servers
//! - **Router** — route API calls from protocol servers to the correct backend
//! - **OneBot v11 parser** — parse raw OneBot v11 events/actions

pub mod adapter;
pub mod bus;
pub mod dedup;
pub mod management;
pub mod middleware;
pub mod onebot_v11;
pub mod router;
pub mod runtime;
pub mod server;
pub mod shared_config;
pub mod stats;
pub mod storage;
