//! Tests for behavior logic and attachment.

use std::time::Duration;

use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

use bigworlds::{behavior::BehaviorHandle, rpc, sim, Result, Signal, SimHandle};

#[allow(unused)]
mod common;

#[tokio::test]
async fn behavior_attachment() -> anyhow::Result<()> {
    // Initialize logging.
    env_logger::init();

    let mut sim = sim::spawn_from(
        common::model(),
        None,
        common::sim_config(),
        CancellationToken::new(),
    )
    .await?;

    let cancel_behavior = CancellationToken::new();

    let _behavior = spawn_behavior(&mut sim, cancel_behavior.clone()).await?;

    sim.step_by(5).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Shut down the behavior and then spawn it again.
    cancel_behavior.cancel();

    let _behavior = spawn_behavior(&mut sim, CancellationToken::new()).await?;

    // Respawned behavior will continue executing the same logic, but note that
    // whatever local state it had is now lost.

    sim.step_by(5).await?;

    sim.shutdown().await?;

    Ok(())
}

/// Shorthand for spawning certain closure-defined behavior on the provided
/// sim instance.
async fn spawn_behavior(sim: &mut SimHandle, cancel: CancellationToken) -> Result<BehaviorHandle> {
    let behavior = sim
        // Spawn a behavior.
        //
        // Behavior is code that runs in it's own task on the runtime.
        // It has raw access to the stream of events coming from the worker.
        // It can also talk directly to the worker using provided executor.
        //
        // Here we spawn a synced behavior from a closure.
        .spawn_behavior_synced(
            |mut stream, _worker| {
                Box::pin(async move {
                    // State is persisted only as long as the behavior task
                    // is alive. 
                    let mut processed_event_count = 0;

                    loop {
                        tokio::select! {
                            Some((sig, s)) = stream.next() => {
                                match sig.payload {
                                    rpc::behavior::Request::Event(_event) => {
                                        processed_event_count += 1;
                                        println!("Processed {} events", processed_event_count);
                                        let _ = s.send(Ok(Signal::new(rpc::behavior::Response::Empty, sig.ctx)));
                                    },
                                    rpc::behavior::Request::Shutdown => {
                                        let _ = s.send(Ok(Signal::new(rpc::behavior::Response::Empty, sig.ctx)));
                                    }
                                }
                            }
                            _ = cancel.cancelled() => {
                                println!("behavior observed cancellation on the passed token...");
                                return Ok(());
                            }
                        }
                    }
                })
            },
            bigworlds::behavior::BehaviorTarget::Worker,
            vec!["step".parse().unwrap()],
        )
        .await?;

    Ok(behavior)
}
