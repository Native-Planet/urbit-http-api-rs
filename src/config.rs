//! Configuration options for the Urbit HTTP API client.

use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Config {
    /// The interval to wait before attempting to reconnect to a dropped SSE connection.
    pub reconnect_interval: Duration,
    /// Optional timeout for HTTP requests.
    pub timeout: Option<Duration>,
    /// Enable debug logging.
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
