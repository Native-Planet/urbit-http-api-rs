//! # Urbit Client
//!
//! This module implements a full-featured Urbit HTTP API client in Rust, mirroring
//! the functionality of the JavaScript version (js-http-api) in an idiomatic Rust style.
//!
//! Features include:
//! - Authentication via ticket (/~/login)
//! - Scry queries (/~/scry)
//! - Poke commands (/~/poke)
//! - Subscription to SSE events (/~/sse) with automatic reconnection
//! - Event emitter for on/off event handling
//! - Message ID tracking for callback matching

use futures::{Stream, StreamExt, TryStreamExt};
use reqwest::{Client, Url};
use serde_json::Value;
use std::{
    collections::HashMap,
    pin::Pin,
    sync::Arc,
};
use tokio::sync::{broadcast, Mutex, oneshot};
use tokio_util::{
    codec::{FramedRead, LinesCodec},
    io::StreamReader,
};
use uuid::Uuid;

use crate::{
    config::Config,
    error::{Error, Result},
    event::UrbitEvent,
};

/// Type alias for the pending call map.
type PendingMap = HashMap<String, oneshot::Sender<Value>>;

/// A client for interacting with an Urbit ship via its HTTP API.
pub struct Urbit {
    /// Base URL of the Urbit ship (e.g., "http://localhost:8080").
    base_url: Url,
    /// Optional ticket for authentication.
    ticket: Option<String>,
    /// The underlying HTTP client.
    client: Client,
    /// Configuration options.
    config: Config,
    /// Event emitter (broadcast channel) for notifying listeners of events.
    event_sender: broadcast::Sender<UrbitEvent>,
    /// Map of pending message callbacks keyed by message ID.
    pending_calls: Arc<Mutex<PendingMap>>,
}

impl Urbit {
    /// Creates a new `Urbit` client with default configuration.
    ///
    /// # Arguments
    ///
    /// * `base_url` - The base URL of the Urbit ship.
    /// * `ticket` - Optional authentication ticket.
    pub fn new<U: Into<String>>(base_url: U, ticket: Option<String>) -> Result<Self> {
        Self::with_config(base_url, ticket, Config::default())
    }

    /// Creates a new `Urbit` client with a custom configuration.
    pub fn with_config<U: Into<String>>(base_url: U, ticket: Option<String>, config: Config) -> Result<Self> {
        let base_url_str = base_url.into();
        let base_url = Url::parse(&base_url_str)
            .map_err(|e| Error::Http(format!("Invalid base URL ({}): {}", base_url_str, e)))?;
        let client_builder = Client::builder().cookie_store(true);
        let client = if let Some(timeout) = config.timeout {
            client_builder.timeout(timeout).build()?
        } else {
            client_builder.build()?
        };
        let (tx, _) = broadcast::channel(100);
        Ok(Self {
            base_url,
            ticket,
            client,
            config,
            event_sender: tx,
            pending_calls: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Constructs a full URL by appending a path to the base URL.
    fn endpoint(&self, path: &str) -> Result<Url> {
        self.base_url
            .join(path)
            .map_err(|e| Error::Http(format!("Error constructing URL: {}", e)))
    }

    /// Authenticates with the Urbit ship using the provided ticket.
    ///
    /// This sends a GET request to `/~/login?ticket=<ticket>` to initiate the session.
    pub async fn connect(&self) -> Result<()> {
        if let Some(ref ticket) = self.ticket {
            let endpoint_path = format!("/~/login?ticket={}", ticket);
            let url = self.endpoint(&endpoint_path)?;
            let resp = self.client.get(url).send().await?;
            if !resp.status().is_success() {
                return Err(Error::Http(format!("Authentication failed with status {}", resp.status())));
            }
            // Emit connected event.
            let _ = self.event_sender.send(UrbitEvent::Connected);
        }
        Ok(())
    }

    /// Performs a scry query on the ship.
    ///
    /// Sends a GET request to `/~/scry/<app>/<path>` with optional query parameters.
    pub async fn scry(&self, app: &str, path: &str, params: Option<Value>) -> Result<Value> {
        let endpoint_path = format!("/~/scry/{}/{}", app, path);
        let url = self.endpoint(&endpoint_path)?;
        let mut req = self.client.get(url);
        if let Some(params) = params {
            if let Some(map) = params.as_object() {
                let query: Vec<(&str, String)> = map.iter().map(|(k, v)| (k.as_str(), v.to_string())).collect();
                req = req.query(&query);
            }
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            return Err(Error::Http(format!("Scry HTTP error: {}", resp.status())));
        }
        let json = resp.json::<Value>().await?;
        Ok(json)
    }

    /// Sends a poke to the ship.
    ///
    /// This sends a POST request to `/~/poke` with the given payload.
    /// A unique message ID is generated and attached to the payload for callback matching.
    pub async fn poke(&self, app: &str, mark: &str, json: Value) -> Result<()> {
        // Generate a unique message ID.
        let msg_id = Uuid::new_v4().to_string();
        let payload = serde_json::json!({
            "action": "poke",
            "app": app,
            "mark": mark,
            "json": json,
            "id": msg_id,
        });
        let endpoint_path = "/~/poke";
        let url = self.endpoint(endpoint_path)?;
        let resp = self.client.post(url).json(&payload).send().await?;
        if !resp.status().is_success() {
            return Err(Error::Http(format!("Poke HTTP error: {}", resp.status())));
        }
        // For full-featured functionality, we might wait for a poke response.
        // This is handled via the subscription stream and callback matching.
        Ok(())
    }

    /// Sends a call to the ship.
    ///
    /// A call is a poke that expects a response. This method returns a future that resolves
    /// when the corresponding response is received.
    pub async fn call(&self, app: &str, mark: &str, json: Value) -> Result<Value> {
        // Generate unique message ID.
        let msg_id = Uuid::new_v4().to_string();
        // Create oneshot channel for response.
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_calls.lock().await;
            pending.insert(msg_id.clone(), tx);
        }
        let payload = serde_json::json!({
            "action": "poke",
            "app": app,
            "mark": mark,
            "json": json,
            "id": msg_id,
        });
        let endpoint_path = "/~/poke";
        let url = self.endpoint(endpoint_path)?;
        let resp = self.client.post(url).json(&payload).send().await?;
        if !resp.status().is_success() {
            return Err(Error::Http(format!("Call HTTP error: {}", resp.status())));
        }
        // Wait for the response via the oneshot channel.
        match rx.await {
            Ok(response) => Ok(response),
            Err(e) => Err(Error::Message(format!("Failed to receive call response: {}", e))),
        }
    }

    /// Subscribes to events from the ship via Server-Sent Events (SSE).
    ///
    /// This spawns a background task that maintains the subscription and automatically
    /// reconnects if the connection is lost.
    pub async fn subscribe(&self, app: &str, event: &str, params: Option<Value>) -> Result<()> {
        let endpoint_path = "/~/sse";
        let url = self.endpoint(endpoint_path)?;
        let mut req = self.client.get(url);
        let mut query_params: Vec<(String, String)> = vec![
            ("app".to_string(), app.to_string()),
            ("path".to_string(), event.to_string()),
        ];
        if let Some(params) = params {
            if let Some(map) = params.as_object() {
                for (k, v) in map.iter() {
                    query_params.push((k.to_string(), v.to_string()));
                }
            }
        }
        req = req.query(&query_params);

        // Clone necessary fields for the background task.
        let client = self.client.clone();
        let url = req
            .try_clone()
            .ok_or_else(|| Error::Http("Failed to clone request".to_string()))?
            .build()
            .map_err(|e| Error::Http(format!("Failed to build cloned request: {}", e)))?
            .url()
            .clone();

        let event_sender = self.event_sender.clone();
        let pending_calls = self.pending_calls.clone();
        let reconnect_interval = self.config.reconnect_interval;
        let debug = self.config.debug;

        // Spawn a background task for maintaining the subscription.
        tokio::spawn(async move {
            loop {
                if debug {
                    println!("Attempting to subscribe to {}", url);
                }
                let res = client.get(url.clone()).send().await;
                match res {
                    Ok(resp) => {
                        if !resp.status().is_success() {
                            let _ = event_sender.send(UrbitEvent::Disconnected);
                        } else {
                            let _ = event_sender.send(UrbitEvent::Connected);
                            // Process the stream.
                            let stream_reader = StreamReader::new(
                                resp.bytes_stream().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)),
                            );
                            let mut lines = FramedRead::new(stream_reader, LinesCodec::new());
                            while let Some(line_result) = lines.next().await {
                                match line_result {
                                    Ok(line) => {
                                        // Try parsing the line as JSON.
                                        match serde_json::from_str::<Value>(&line) {
                                            Ok(json) => {
                                                // Emit raw message event.
                                                let _ = event_sender.send(UrbitEvent::RawMessage(json.clone()));
                                                // Check if the message has an "id" field for pending calls.
                                                if let Some(id_val) = json.get("id") {
                                                    if let Some(id) = id_val.as_str() {
                                                        let mut pending = pending_calls.lock().await;
                                                        if let Some(tx) = pending.remove(id) {
                                                            let _ = tx.send(json.clone());
                                                            let _ = event_sender.send(UrbitEvent::PokeResponse { id: id.to_string(), response: json.clone() });
                                                            continue;
                                                        }
                                                    }
                                                }
                                                // Emit subscription message event.
                                                let _ = event_sender.send(UrbitEvent::SubscriptionMessage(json));
                                            }
                                            Err(e) => {
                                                if debug {
                                                    eprintln!("Failed to parse JSON: {}", e);
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        if debug {
                                            eprintln!("Error reading line from SSE: {}", e);
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if debug {
                            eprintln!("Subscription error: {}", e);
                        }
                        let _ = event_sender.send(UrbitEvent::Disconnected);
                    }
                }
                // Wait for reconnect interval before trying again.
                tokio::time::sleep(reconnect_interval).await;
                let _ = event_sender.send(UrbitEvent::Reconnected);
            }
        });

        Ok(())
    }

    /// Returns a new receiver for events.
    ///
    /// This allows consumers to use an "on" style API to listen for events.
    pub fn subscribe_events(&self) -> broadcast::Receiver<UrbitEvent> {
        self.event_sender.subscribe()
    }
}
