//! # Urbit Client
//!
//! This module implements a full-featured Urbit HTTP API client.
//! It mirrors the functionality of the JavaScript Urbit class, supporting:
//! - Authentication (via `connect()`)
//! - Scry queries
//! - Poke commands with callbacks (onSuccess and onError)
//! - Call (a poke expecting a response)
//! - Subscriptions via SSE (with auto-reconnection)
//! - An event emitter ("on"/"off") API for handling events
//! - Thread execution and channel reset functionality
//!
//! The SSE parser aggregates lines until a blank line is encountered,
//! strips off the `"data:"`, `"id:"`, and `"retry:"` prefixes, then
//! concatenates data lines into one JSON payload which is parsed and used
//! to invoke poke callbacks (ack or nack).

use futures::{StreamExt, TryStreamExt};
use reqwest::Client;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::{broadcast, Mutex};
use tokio_util::codec::{FramedRead, LinesCodec};
use tokio_util::io::StreamReader;

use crate::{
    config::Config,
    error::{Error, Result},
    event::{ChannelStatus, ErrorEvent, FactEvent, ResetEvent, StatusUpdateEvent, SubscriptionEvent, UrbitEvent},
    utils::{hex_string, EventEmitter},
};

/// Structure representing a complete SSE event.
#[derive(Debug)]
struct SseEvent {
    pub id: Option<String>,
    pub event: Option<String>,
    pub data: Option<String>,
    pub retry: Option<u64>,
}

/// Reads lines from the provided FramedRead until a blank line is encountered,
/// then constructs and returns a complete SseEvent.
async fn read_sse_event<R>(reader: &mut FramedRead<R, LinesCodec>) -> Option<SseEvent>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut id = None;
    let mut event = None;
    let mut data_lines: Vec<String> = Vec::new();
    let mut retry = None;

    while let Some(line_result) = reader.next().await {
        match line_result {
            Ok(line) => {
                let line = line.trim().to_string();
                if line.is_empty() {
                    // End of event block.
                    break;
                }
                if line.starts_with("id:") {
                    id = Some(line["id:".len()..].trim().to_string());
                } else if line.starts_with("event:") {
                    event = Some(line["event:".len()..].trim().to_string());
                } else if line.starts_with("data:") {
                    data_lines.push(line["data:".len()..].trim().to_string());
                } else if line.starts_with("retry:") {
                    if let Ok(r) = line["retry:".len()..].trim().parse::<u64>() {
                        retry = Some(r);
                    }
                }
            }
            Err(_) => return None,
        }
    }
    if data_lines.is_empty() && id.is_none() && event.is_none() {
        return None;
    }
    let data = if data_lines.is_empty() {
        None
    } else {
        Some(data_lines.join("\n"))
    };
    Some(SseEvent { id, event, data, retry })
}

/// Handlers for poke responses.
pub struct PokeHandlers {
    pub on_success: Box<dyn Fn() + Send + Sync>,
    pub on_error: Box<dyn Fn(String) + Send + Sync>,
}

/// Handlers for subscription events.
pub struct SubscriptionRequest {
    pub app: String,
    pub path: String,
    pub resub_on_quit: bool,
    pub event: Box<dyn Fn(Value, String, u32) + Send + Sync>,
    pub err: Box<dyn Fn(String, u32) + Send + Sync>,
    pub quit: Box<dyn Fn(Value) + Send + Sync>,
}

/// The main Urbit client struct.
pub struct Urbit {
    /// Base URL of the ship (e.g. "http://localhost:8080")
    pub url: String,
    /// Optional access code
    pub code: Option<String>,
    /// Optional desk (used for thread calls)
    pub desk: Option<String>,
    /// Optional verbose flag
    pub verbose: bool,

    /// Underlying HTTP client (with cookie store enabled)
    client: Client,
    /// Configuration
    config: Config,
    /// Broadcast channel for events
    event_sender: broadcast::Sender<UrbitEvent>,
    /// Internal event emitter (for debugging)
    emitter: EventEmitter,
    /// Map of outstanding poke callbacks (keyed by message id)
    outstanding_pokes: Arc<Mutex<HashMap<u32, PokeHandlers>>>,
    /// Map of outstanding subscription callbacks (keyed by message id)
    outstanding_subscriptions: Arc<Mutex<HashMap<u32, SubscriptionRequest>>>,
    /// Used to generate unique IDs for events
    last_event_id: Mutex<u32>,
    /// The unique channel UID (generated from current time and a random hex string)
    uid: Arc<Mutex<String>>,
    /// Cookie for non-browser contexts
    pub cookie: Option<String>,
    /// The ship name (set after authentication)
    pub ship: Option<String>,
    /// Our identity (set after authentication)
    pub our: Option<String>,
    /// Count of consecutive errors (for reconnection backoff)
    error_count: Mutex<u32>,
}

impl Urbit {
    /// Creates a new Urbit client with default configuration.
    pub fn new<U: Into<String>>(url: U, code: Option<String>) -> Result<Self> {
        Self::with_config(url, code, Config::default())
    }

    /// Creates a new Urbit client with custom configuration.
    pub fn with_config<U: Into<String>>(url: U, code: Option<String>, config: Config) -> Result<Self> {
        let client = Client::builder().cookie_store(true).build()?;
        let (tx, _) = broadcast::channel(100);
        Ok(Self {
            url: url.into(),
            code,
            desk: None,
            verbose: false,
            client,
            config,
            event_sender: tx,
            emitter: EventEmitter::new(),
            outstanding_pokes: Arc::new(Mutex::new(HashMap::new())),
            outstanding_subscriptions: Arc::new(Mutex::new(HashMap::new())),
            last_event_id: Mutex::new(0),
            uid: Arc::new(Mutex::new(Self::generate_uid())),
            cookie: None,
            ship: None,
            our: None,
            error_count: Mutex::new(0),
        })
    }

    /// Generates a unique channel UID.
    fn generate_uid() -> String {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let hex = hex_string(6);
        format!("{}-{}", secs, hex)
    }

    /// Asynchronously computes the channel URL.
    pub async fn channel_url(&self) -> String {
        let uid = self.uid.lock().await.clone();
        format!("{}/~/channel/{}", self.url, uid)
    }

    /// Increments and returns a new event id.
    async fn get_event_id(&self) -> u32 {
        let mut id = self.last_event_id.lock().await;
        *id += 1;
        *id
    }

    /// Emits an event via the broadcast channel.
    fn emit(&self, event: UrbitEvent) {
        if self.verbose {
            println!("Emitting event: {}", event);
        }
        let _ = self.event_sender.send(event);
    }

    /// Registers a listener for events.
    pub fn on(&self, callback: impl Fn(UrbitEvent) + Send + Sync + 'static) {
        let mut rx = self.event_sender.subscribe();
        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                callback(event);
            }
        });
    }

    /// Connects to the Urbit ship by sending a login POST request.
    pub async fn connect(&mut self) -> Result<()> {
        if self.verbose {
            println!("Connecting to {} with code {:?}", self.url, self.code);
        }
        let body = format!("password={}", self.code.clone().unwrap_or_default());
        let resp = self.client
            .post(&format!("{}/~/login", self.url))
            .body(body)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Error::Http(format!("Login failed with status {}", resp.status())));
        }
        if let Some(cookie) = resp.headers().get("set-cookie") {
            self.cookie = Some(cookie.to_str().unwrap_or_default().to_string());
        }
        self.get_ship_name().await;
        self.get_our_name().await;
        self.emit(UrbitEvent::Connected);
        Ok(())
    }

    /// Starts the SSE event source.
    pub async fn event_source(&self) -> Result<()> {
        let channel_url = self.channel_url().await;
        if self.verbose {
            println!("Starting event source from {}", channel_url);
        }
        let client = self.client.clone();
        let event_sender = self.event_sender.clone();
        let outstanding_pokes = self.outstanding_pokes.clone();
        let verbose = self.verbose;
        let uid = Arc::clone(&self.uid);
        let reconnect_interval = self.config.reconnect_interval;
        tokio::spawn(async move {
            loop {
                let resp = client.get(&channel_url).send().await;
                match resp {
                    Ok(resp) => {
                        if !resp.status().is_success() {
                            let _ = event_sender.send(UrbitEvent::Disconnected);
                        } else {
                            let _ = event_sender.send(UrbitEvent::StatusUpdate(StatusUpdateEvent {
                                status: ChannelStatus::Active,
                            }));
                            let stream = resp.bytes_stream();
                            let stream_reader = StreamReader::new(
                                stream.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)),
                            );
                            let mut lines = FramedRead::new(stream_reader, LinesCodec::new());
                            while let Some(sse_event) = read_sse_event(&mut lines).await {
                                if verbose {
                                    println!("Parsed SSE event: {:?}", sse_event);
                                }
                                if let Some(data_str) = sse_event.data {
                                    if let Ok(json) = serde_json::from_str::<Value>(&data_str) {
                                        // Check if this event is a poke response.
                                        if let Some(resp_type) = json.get("response").and_then(|v| v.as_str()) {
                                            if resp_type == "poke" {
                                                if let Some(id_val) = json.get("id").and_then(|v| v.as_u64()) {
                                                    let id_u32 = id_val as u32;
                                                    let mut pokes = outstanding_pokes.lock().await;
                                                    if let Some(handlers) = pokes.remove(&id_u32) {
                                                        if json.get("ok").is_some() {
                                                            (handlers.on_success)();
                                                        } else if let Some(err) = json.get("err").and_then(|v| v.as_str()) {
                                                            (handlers.on_error)(err.to_string());
                                                        } else {
                                                            eprintln!("Invalid poke response: {:?}", json);
                                                        }
                                                        continue;
                                                    }
                                                }
                                            }
                                        }
                                        let _ = event_sender.send(UrbitEvent::Fact(FactEvent {
                                            id: 0,
                                            time: chrono::Utc::now().timestamp_millis() as u64,
                                            data: json.clone(),
                                        }));
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if verbose {
                            eprintln!("Event source error: {}", e);
                        }
                        let _ = event_sender.send(UrbitEvent::Error(ErrorEvent {
                            time: chrono::Utc::now().timestamp_millis() as u64,
                            msg: e.to_string(),
                        }));
                    }
                }
                tokio::time::sleep(reconnect_interval).await;
                let new_uid = Self::generate_uid();
                {
                    let mut uid_lock = uid.lock().await;
                    *uid_lock = new_uid;
                }
                let _ = event_sender.send(UrbitEvent::Reconnected);
            }
        });
        Ok(())
    }

    /// Fetches the ship name from `${url}/~/host` and stores it.
    pub async fn get_ship_name(&mut self) {
        if self.ship.is_some() {
            return;
        }
        if let Ok(resp) = self.client.get(format!("{}/~/host", self.url)).send().await {
            if let Ok(text) = resp.text().await {
                self.ship = Some(text[1..].to_string());
            }
        }
    }

    /// Fetches our name from `${url}/~/name` and stores it.
    pub async fn get_our_name(&mut self) {
        if self.our.is_some() {
            return;
        }
        if let Ok(resp) = self.client.get(format!("{}/~/name", self.url)).send().await {
            if let Ok(text) = resp.text().await {
                self.our = Some(text[1..].to_string());
            }
        }
    }

    /// Performs a scry request.
    pub async fn scry(&self, app: &str, path: &str, params: Option<Value>) -> Result<Value> {
        let mut url = format!("{}/~/scry/{}{}", self.url, app, path);
        url.push_str(".json");
        let mut req = self.client.get(&url);
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

    /// Sends a poke command with user-supplied callbacks.
    pub async fn poke(
        &mut self,
        app: &str,
        mark: &str,
        json: Value,
        on_success: impl Fn() + Send + Sync + 'static,
        on_error: impl Fn(String) + Send + Sync + 'static,
    ) -> Result<u32> {
        let id = self.get_event_id().await;
        let message = serde_json::json!({
            "id": id,
            "action": "poke",
            "ship": self.ship.clone().unwrap_or_default(),
            "app": app,
            "mark": mark,
            "json": json
        });
        {
            let mut pokes = self.outstanding_pokes.lock().await;
            pokes.insert(id, PokeHandlers {
                on_success: Box::new(on_success),
                on_error: Box::new(on_error),
            });
        }
        match self.send_json_to_channel(&message).await {
            Ok(_) => Ok(id),
            Err(e) => {
                let mut pokes = self.outstanding_pokes.lock().await;
                if let Some(handlers) = pokes.remove(&id) {
                    (handlers.on_error)(e.to_string());
                }
                Err(e)
            }
        }
    }

    /// Sends JSON to the channel via a PUT request.
    /// The message is wrapped in an array, as expected by the ship.
    async fn send_json_to_channel(&self, json: &Value) -> Result<()> {
        let channel_url = self.channel_url().await;
        let payload = serde_json::to_string(&vec![json])?;
        let resp = self.client
            .put(&channel_url)
            .header("Content-Type", "application/json")
            .body(payload)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Error::Http(format!("Failed to PUT channel: {}", resp.status())));
        }
        Ok(())
    }

    /// Subscribes to a given app/path with explicit parameters.
    pub async fn subscribe(
        &mut self,
        app: &str,
        path: &str,
        resub_on_quit: bool,
        event_cb: impl Fn(Value, String, u32) + Send + Sync + 'static,
        err_cb: impl Fn(String, u32) + Send + Sync + 'static,
        quit_cb: impl Fn(Value) + Send + Sync + 'static,
    ) -> Result<u32> {
        if *self.last_event_id.lock().await == 0 {
            self.emit(UrbitEvent::StatusUpdate(StatusUpdateEvent {
                status: ChannelStatus::Opening,
            }));
        }
        let id = self.get_event_id().await;
        let message = serde_json::json!({
            "id": id,
            "action": "subscribe",
            "ship": self.ship.clone().unwrap_or_default(),
            "app": app,
            "path": path,
        });
        {
            let mut subs = self.outstanding_subscriptions.lock().await;
            subs.insert(id, SubscriptionRequest {
                app: app.to_string(),
                path: path.to_string(),
                resub_on_quit,
                event: Box::new(event_cb),
                err: Box::new(err_cb),
                quit: Box::new(quit_cb),
            });
        }
        self.emit(UrbitEvent::Subscription(SubscriptionEvent {
            id,
            app: Some(app.to_string()),
            path: Some(path.to_string()),
            status: "open".into(),
        }));
        self.send_json_to_channel(&message).await?;
        Ok(id)
    }

    /// Unsubscribes a subscription given its id.
    pub async fn unsubscribe(&mut self, subscription: u32) -> Result<()> {
        let msg_id = self.get_event_id().await;
        let message = serde_json::json!({
            "id": msg_id,
            "action": "unsubscribe",
            "subscription": subscription,
        });
        self.send_json_to_channel(&message).await?;
        {
            let mut subs = self.outstanding_subscriptions.lock().await;
            subs.remove(&subscription);
        }
        self.emit(UrbitEvent::Subscription(SubscriptionEvent {
            id: subscription,
            app: None,
            path: None,
            status: "close".into(),
        }));
        Ok(())
    }

    /// Deletes the channel.
    pub async fn delete(&mut self) -> Result<()> {
        let msg_id = self.get_event_id().await;
        let body = serde_json::to_string(&[serde_json::json!({
            "id": msg_id,
            "action": "delete",
        })])?;
        let channel_url = self.channel_url().await;
        #[cfg(target_arch = "wasm32")]
        {
            // In browser, use navigator.sendBeacon (not implemented here)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let resp = self.client
                .post(&channel_url)
                .header("Content-Type", "application/json")
                .body(body)
                .send()
                .await?;
            if !resp.status().is_success() {
                return Err(Error::Http(format!("Failed to DELETE channel: {}", resp.status())));
            }
        }
        Ok(())
    }

    /// Runs a thread by POSTing to a constructed URL.
    pub async fn thread<R: serde::de::DeserializeOwned, T: serde::Serialize>(
        &self,
        input_mark: &str,
        output_mark: &str,
        thread_name: &str,
        body: T,
    ) -> Result<R> {
        let desk = self.desk.as_ref().ok_or_else(|| Error::Message("Must supply desk to run thread from".into()))?;
        let url = format!("{}/spider/{}/{}/{}/{}.json", self.url, desk, input_mark, thread_name, output_mark);
        let resp = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&body)?)
            .send()
            .await?;
        Ok(resp.json::<R>().await?)
    }

    /// Resets the connection: aborts, clears subscriptions and outstanding pokes, and regenerates uid.
    pub async fn reset(&mut self) {
        self.delete().await.ok();
        {
            let mut id_lock = self.last_event_id.lock().await;
            *id_lock = 0;
        }
        {
            let mut uid_lock = self.uid.lock().await;
            *uid_lock = Self::generate_uid();
        }
        {
            let mut subs = self.outstanding_subscriptions.lock().await;
            subs.clear();
        }
        {
            let mut pokes = self.outstanding_pokes.lock().await;
            pokes.clear();
        }
        let new_uid = { self.uid.lock().await.clone() };
        self.emit(UrbitEvent::Reset(ResetEvent { uid: new_uid }));
    }

    /// Static helper: Authenticates and initializes an Urbit connection.
    pub async fn authenticate(ship: &str, url: &str, code: &str, verbose: bool) -> Result<Self> {
        let mut airlock = Urbit::new(url, Some(code.to_string()))?;
        airlock.verbose = verbose;
        airlock.ship = Some(ship.to_string());
        airlock.connect().await?;
        // Send an initial poke to open the channel.
        airlock.poke(
            "hood",
            "helm-hi",
            serde_json::json!("opening airlock"),
            || { println!("Initial poke succeeded."); },
            |err| { eprintln!("Initial poke error: {}", err); }
        ).await?;
        airlock.event_source().await?;
        Ok(airlock)
    }

    /// Static helper: Connects to a ship on the Arvo network.
    pub async fn on_arvo_network(ship: &str, code: &str) -> Result<Self> {
        let url = format!("https://{}.arvo.network", ship);
        Self::authenticate(ship, &url, code, false).await
    }
}