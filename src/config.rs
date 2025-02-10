//! Configuration options for the Urbit HTTP API client.

use std::time::Duration;

/// Configuration for the Urbit client.
#[derive(Clone, Debug)]
pub struct Config {
    /// Reconnect interval for SSE subscription.
    pub reconnect_interval: Duration,
    /// Optional timeout for HTTP requests.
    pub timeout: Option<Duration>,
    /// Debug mode flag.
    pub debug: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            reconnect_interval: Duration::from_secs(5),
            timeout: None,
            debug: false,
        }
    }
}
