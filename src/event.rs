//! Event definitions for the Urbit HTTP API client.

use serde_json::Value;
use std::fmt;

/// Enumeration of events emitted by the Urbit client.
#[derive(Clone, Debug)]
pub enum UrbitEvent {
    /// Emitted when the client successfully connects/authenticates.
    Connected,
    /// Emitted when the connection is lost.
    Disconnected,
    /// Emitted when the client reconnects after a disconnection.
    Reconnected,
    /// Emitted when a subscription message is received.
    SubscriptionMessage(Value),
    /// Emitted for poke responses matched by message ID.
    PokeResponse { id: String, response: Value },
    /// Emitted for any raw message.
    RawMessage(Value),
}

impl fmt::Display for UrbitEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UrbitEvent::Connected => write!(f, "Connected"),
            UrbitEvent::Disconnected => write!(f, "Disconnected"),
            UrbitEvent::Reconnected => write!(f, "Reconnected"),
            UrbitEvent::SubscriptionMessage(val) => write!(f, "SubscriptionMessage: {}", val),
            UrbitEvent::PokeResponse { id, response } => write!(f, "PokeResponse {}: {}", id, response),
            UrbitEvent::RawMessage(val) => write!(f, "RawMessage: {}", val),
        }
    }
}
