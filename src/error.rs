use serde::{ser::Serializer, Serialize};
use thiserror::Error;

/// A specialized `Result` type for the Urbit HTTP API crate.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for the Urbit HTTP API crate.
#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Serde JSON error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("Message error: {0}")]
    Message(String),

    #[error("Broadcast error: {0}")]
    Broadcast(#[from] tokio::sync::broadcast::error::RecvError),
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
