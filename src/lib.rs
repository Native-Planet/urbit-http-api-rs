//! # Urbit HTTP API
//!
//! This crate is a full-featured, idiomatic Rust port of the Urbit HTTP API,
//! mirroring the functionality of the JavaScript version (js-http-api).
//!
//! It supports authentication, scry, poke, event subscription with automatic reconnection,
//! message ID and callback matching, and an event emitter pattern.
//!
//! For usage details, see the example in `examples/simple.rs`.

pub mod config;
pub mod error;
pub mod event;
pub mod urbit;
