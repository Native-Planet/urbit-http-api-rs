//! # Urbit HTTP API
//!
//! This crate is a full-featured, idiomatic Rust port of the Urbit HTTP API,
//! mirroring the functionality of the JavaScript version. It supports:
//!
//! - Authentication (via `connect()`)
//! - Scry queries
//! - Poke commands (with callbacks for success and error)
//! - Call (a poke expecting a response)
//! - Subscriptions via SSE (with auto‑reconnection)
//! - An event emitter ("on"/"off") API for handling events
//! - Thread execution and channel reset functionality
//!
//! For usage examples, see `examples/simple.rs`.

pub mod config;
pub mod error;
pub mod event;
pub mod utils;
pub mod urbit;
