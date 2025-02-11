//! Event definitions for the Urbit HTTP API client.

use serde::Serialize;
use std::fmt;

#[derive(Clone, Debug, Serialize)]
pub enum ChannelStatus {
    Initial,
    Opening,
    Active,
    Reconnecting,
    Reconnected,
    Errored,
}

#[derive(Clone, Debug, Serialize)]
pub struct StatusUpdateEvent {
    pub status: ChannelStatus,
}

#[derive(Clone, Debug, Serialize)]
pub struct FactEvent {
    pub id: u32,
    pub time: u64,
    pub data: serde_json::Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct SubscriptionEvent {
    pub id: u32,
    pub app: Option<String>,
    pub path: Option<String>,
    pub status: String, // e.g., "open" or "close"
}

#[derive(Clone, Debug, Serialize)]
pub struct ErrorEvent {
    pub time: u64,
    pub msg: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ResetEvent {
    pub uid: String,
}

/// Top-level event enum.
#[derive(Clone, Debug)]
pub enum UrbitEvent {
    Connected,
    Disconnected,
    Reconnected,
    StatusUpdate(StatusUpdateEvent),
    Fact(FactEvent),
    Subscription(SubscriptionEvent),
    Error(ErrorEvent),
    Reset(ResetEvent),
}

impl fmt::Display for UrbitEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UrbitEvent::Connected => write!(f, "Connected"),
            UrbitEvent::Disconnected => write!(f, "Disconnected"),
            UrbitEvent::Reconnected => write!(f, "Reconnected"),
            UrbitEvent::StatusUpdate(e) => write!(f, "StatusUpdate: {:?}", e),
            UrbitEvent::Fact(e) => write!(f, "Fact: {:?}", e),
            UrbitEvent::Subscription(e) => write!(f, "Subscription: {:?}", e),
            UrbitEvent::Error(e) => write!(f, "Error: {:?}", e),
            UrbitEvent::Reset(e) => write!(f, "Reset: {:?}", e),
        }
    }
}
