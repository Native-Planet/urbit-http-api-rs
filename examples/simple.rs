use clap::Parser;
use serde_json::json;
use urbit_http_api_rs::error::Result;
use urbit_http_api_rs::urbit::Urbit;

/// Simple program to test Urbit HTTP API: connect and poke.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The URL of the Urbit ship (e.g., http://localhost)
    #[arg(short, long)]
    url: String,

    /// The access code to use for connecting
    #[arg(short, long)]
    code: String,

    /// Enable verbose logging
    #[arg(short, long, action = clap::ArgAction::SetTrue, default_value_t = false)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Create a new Urbit client using the provided URL and code.
    let mut urbit = Urbit::new(&args.url, Some(args.code.clone()))?;
    // Optionally enable verbose logging.
    urbit.verbose = args.verbose;

    // Connect to the ship.
    urbit.connect().await?;
    println!("Connected to the ship.");

    // Start the SSE event source so that incoming ack/nack events are processed.
    urbit.event_source().await?;
    println!("Event source started.");

    urbit.get_ship_name().await;
    println!("Ship name: {:?}", urbit.ship);
    urbit.get_our_name().await;
    println!("Our name: {:?}", urbit.our);

    // Poke: Send a command with callbacks.
    let poke_id = urbit.poke(
        "hood",
        "helm-hi", 
        json!("Test payload"),
        || {
            println!("Poke succeeded");
        },
        |err| {
            println!("Poke failed: {}", err);
        }
    ).await?;
    println!("Poke sent with id: {}", poke_id);

    // Scry: Query the ship’s state.
    // let scry_result = urbit.scry("groups", "/keys", None).await?;
    // println!("Scry result: {:#?}", scry_result);
  
    // Subscribe: Start a subscription.
    //   let sub_id = urbit.subscribe(
    //       "graph-store",
    //       "/updates",
    //       false, // don't automatically resubscribe on quit
    //       |data, mark, id| {
    //           println!("Subscription event [id={}, mark={}]: {:?}", id, mark, data);
    //       },
    //       |err, id| {
    //           eprintln!("Subscription error [id={}]: {}", id, err);
    //       },
    //       |data| {
    //           println!("Subscription quit with data: {:?}", data);
    //       }
    //   ).await?;
    //   println!("Subscription started with id: {}", sub_id);
  
    // Thread: Run a thread (example; adjust input/output marks and thread name as appropriate).
    //   let thread_result: serde_json::Value = urbit.thread("input-mark", "output-mark", "example-thread", json!({"key": "value"})).await?;
    //   println!("Thread result: {:#?}", thread_result);
  
  
    // Wait some time to allow the SSE event to arrive and the callbacks to be invoked.
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    // Unsubscribe from the subscription.
    //   urbit.unsubscribe(sub_id).await?;
    //   println!("Unsubscribed from subscription id: {}", sub_id);

    Ok(())
}