use futures::StreamExt;
use serde_json::json;
use urbit_http_api_rs::error::Result;
use urbit_http_api_rs::urbit::Urbit;

#[tokio::main]
async fn main() -> Result<()> {
    // Replace with your actual ship's URL and ticket.
    let base_url = "http://localhost";
    let ticket = Some("lidlut-tabwed-pillex-ridrup".to_string());

    // Create an Urbit client with default configuration.
    let api = Urbit::new(base_url, ticket)?;

    // Get a receiver for events (the "on" API).
    let mut events_rx = api.subscribe_events();

    // Connect (authenticate) with the ship.
    api.connect().await?;
    println!("Connected to Urbit ship!");

    // Scry example.
    // let scry_response = api.scry("app-name", "scry-path", Some(json!({ "sample": "data" }))).await?;
    // println!("Scry response: {:#?}", scry_response);

    // Poke example with a plain string payload.
    let poke_payload = json!("This is a plain string payload");
    api.poke("hood", "helm-hi", poke_payload).await?;
    println!("Poke sent successfully with a plain string payload!");

    // Call example (a poke that expects a response).
    // let call_response = api.call("app-name", "poke-mark", json!("Call payload")).await?;
    // println!("Call response received: {:#?}", call_response);

    // Start subscription to events.
    // api.subscribe("app-name", "event-path", None).await?;
    // println!("Subscribed to events. Waiting for events...");

    // Process some events.
    // for _ in 0..10 {
    //     if let Ok(event) = events_rx.recv().await {
    //         println!("Event: {}", event);
    //     }
    // }

    Ok(())
}
