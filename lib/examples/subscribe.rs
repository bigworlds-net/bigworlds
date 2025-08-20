//! Subscription example.

use std::time::Duration;

use tokio_util::sync::CancellationToken;

use bigworlds::{
    client::AsyncClient,
    query::{self, Trigger},
    rpc::msg::Message,
    sim::{self, SimConfig},
    Query, QueryProduct,
};

mod common;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging.
    env_logger::init();

    // Spawn local sim instance including a server interface.
    let mut sim = sim::spawn_from(
        common::model(),
        None,
        SimConfig {
            server: Some(bigworlds::ServerConfig {
                // ALT: declare an actual network listener.
                // listeners: vec!["127.0.0.1:9123".parse()?],
                ..Default::default()
            }),
            ..Default::default()
        },
        CancellationToken::new(),
    )
    .await?;

    // Operate the sim handle as a client through the server interface.
    let mut client = sim.clone().as_client();

    // ALT: utilize a networked client instead.
    // let mut client = Client::connect(
    //     "127.0.0.1:9123".parse()?,
    //     client::Config {
    //         is_blocking: true,
    //         ..Default::default()
    //     },
    // )
    // .await?;

    let query = Query::default().map(query::Map::All);

    // Register a basic query triggered on each step event.
    let mut msg_stream = client
        .subscribe(vec![Trigger::StepEvent], query.clone())
        .await?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<QueryProduct>();

    // Spawn a new task to process incoming subscription messages.
    let client_ = client.clone();
    tokio::spawn(async move {
        while let Ok(Some(message)) = msg_stream.recv().await {
            println!("client: got response from stream");
            match message {
                Message::SubscribeResponse(id) => {
                    let mut client_ = client_.clone();

                    // Once we confirm the subscription was registered
                    // successfully, spawn a new task that will unsubscribe
                    // after a set amount of time.
                    //
                    // Removing a registered subscription requires providing
                    // the id received with the initial response to the
                    // subscription registration.
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        client_.unsubscribe(id).await.unwrap();
                        println!("unsubscribed");
                    });
                }
                Message::QueryResponse(product) => {
                    println!("client: got product");
                    tx.send(product).unwrap();
                }
                _ => unimplemented!(),
            }
        }
        println!("client: end of messages, quitting...");
    });

    // Step through and confirm our subscription msg stream is indeed receiving
    // query products.
    sim.step().await?;
    assert_eq!(Some(sim.query(query).await?), rx.recv().await);

    tokio::time::sleep(Duration::from_secs(1)).await;
    sim.step().await?;
    let _ = rx.recv().await;

    // Step for the last time after some time to confirm the subscription was
    // cancelled and the task receiving from the msg stream has quit.
    tokio::time::sleep(Duration::from_secs(2)).await;
    sim.step().await?;
    assert!(rx.recv().await.is_none());

    Ok(())
}
