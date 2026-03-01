//! # ferroq-core
//!
//! Core types, traits, and abstractions for the ferroq QQ Bot unified gateway.
//!
//! This crate contains no I/O logic — only data structures, error types, and trait
//! definitions that the rest of the workspace depends on.

pub mod adapter;
pub mod api;
pub mod config;
pub mod error;
pub mod event;
pub mod message;
pub mod protocol;
