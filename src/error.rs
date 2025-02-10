use serde::{ser::Serializer, Serialize};

/// A specialized `Result` type for the Urbit HTTP API crate.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for the Urbit HTTP API crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Message processing error: {0}")]
    Message(String),

    #[error("Event channel error: {0}")]
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
