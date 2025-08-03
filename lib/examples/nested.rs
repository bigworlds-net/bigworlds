//! This example showcases basic simulation nesting.

use tokio_stream::StreamExt;

use bigworlds::{
    behavior::BehaviorTarget,
    rpc,
    sim::{self, SimConfig},
    string, Error, Signal,
};
use tokio_util::sync::CancellationToken;

mod common;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging.
    env_logger::init();

    let cancel = CancellationToken::new();

    let mut sim = sim::spawn_from(
        common::model(),
        None,
        SimConfig::default(),
        CancellationToken::new(),
    )
    .await?;

    let mut behaviors = vec![];

    for _ in 0..3 {
        let cancel = cancel.clone();
        // Spawn nested simulation instance accessible only to
        // the behavior.
        //
        // NOTE: as we move this object into the behavior closure, it's
        // going to live only as long as the behavior lives. Persisting the
        // nested simulation state would require serializing it into bytes and
        // storing as a variable.
        let mut nested_sim = sim.fork().await?;
        let behavior = sim
            // Spawn top-level behavior.
            .spawn_behavior_synced(
                |mut stream, _worker| {
                    Box::pin(async move {
                        // On the nested sim we spawn a new machine behavior. 
                        //
                        // NOTE: we can specify triggers same as the top-level
                        // sim but they are not propagated from the top level
                        // sim by default. We would have to explicitly pass
                        // events to make it work.
                        let machine = nested_sim.spawn_machine(None, vec![string::new_truncate("step")]).await?;
                        // Execute a spawn command, spawning a new entity based
                        // on the prefab our fork has inherited from the
                        // original model.
                        let cube_name = "nested_cube_0";
                        machine
                            .execute_cmd(bigworlds::machine::Command::Spawn(bigworlds::machine::cmd::spawn::Spawn {
                                prefab: Some(string::new_truncate("cube")),
                                spawn_name: Some(string::new_truncate(cube_name)),
                                out: None,
                            }))
                            .await?;

                        assert_eq!(nested_sim.entities().await?.iter().any(|e| e.as_str() == cube_name), true);

                        loop {
                            tokio::select! {
                                Some((sig, s)) = stream.next() => {
                                    match sig.payload {
                                        rpc::behavior::Request::Event(event) => {
                                            println!("behavior: received trigger: {event}");
                                            match event.as_str() {
                                                "step" => nested_sim.step_by(10).await?,
                                                _ => (),
                                            }
                                            let _ = s.send(Ok(Signal::new(rpc::behavior::Response::Empty, sig.ctx)));
                                        },
                                        rpc::behavior::Request::Shutdown => {
                                            println!("behavior: received shutdown");
                                            nested_sim.shutdown().await?;
                                            let _ = s.send(Ok(Signal::new(rpc::behavior::Response::Empty, sig.ctx)));
                                        }
                                        _ => {
                                            let _ = s.send(Err(Error::Other("not implemented".to_owned())));
                                        },
                                    }
                                }
                                _ = cancel.cancelled() => {
                                    println!("behavior observed cancellation on the passed token...");
                                    nested_sim.shutdown().await?;
                                    return Ok(());
                                }
                            }
                        }
                    })
                },
                BehaviorTarget::Worker,
                vec!["step".parse()?],
            )
            .await?;
        behaviors.push(behavior);
    }

    println!("behaviors spawned");

    // let r = behaviors[0]
    //     .execute(Signal::from(rpc::behavior::Request::Event(
    //         "custom_event0".parse()?,
    //     )))
    //     .await??;
    // println!("behavior: custom event trigger sent, response: {r:?}");

    sim.step().await?;
    sim.step().await?;

    sim.shutdown().await?;

    Ok(())
}
