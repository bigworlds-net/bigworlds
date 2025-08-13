//! This example showcases basic simulation nesting.

use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

use bigworlds::{
    behavior::BehaviorTarget,
    rpc,
    sim::{self, SimConfig},
    Signal,
};

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

    for n in 0..2 {
        let cancel = cancel.clone();
        // Spawn nested simulation instance accessible only to
        // the behavior.
        //
        // NOTE: as we move this object into the behavior closure, it's
        // going to live only as long as the behavior lives. Persisting the
        // nested simulation state would require serializing it into bytes and
        // storing as a variable.
        let mut nested_sim = sim.fork().await?;

        // Leave only one cube entity per nested sim instance.
        nested_sim.remove_entity("cube_1").await?;
        nested_sim.remove_entity("cube_2").await?;

        // Insert a custom event per nested simulation instance.

        // Modify the behaviors per nested simulation instance.
        let mut model = nested_sim.get_model().await?;
        model.behaviors.iter_mut().for_each(|b| {
            if let bigworlds::model::behavior::BehaviorInner::Lua { script, .. } = &mut b.inner {
                *script = format!(r#"print("hello from nested model #{} !")"#, n);
            }
        });
        nested_sim.pull_model(model).await?;

        let nested_event = format!("nested_event_{}", n);
        let nested_event_ = nested_event.clone();

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
                        let machine = nested_sim.spawn_machine(None, vec!["step".to_owned()]).await?;
                        // Execute a spawn command, spawning a new entity based
                        // on the prefab our fork has inherited from the
                        // original model.
                        let cube_name = "nested_cube_0";
                        machine
                            .execute_cmd(bigworlds::machine::Command::Spawn(bigworlds::machine::cmd::spawn::Spawn {
                                prefab: Some("cube".to_owned()),
                                spawn_name: Some(cube_name.to_owned()),
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
                                                "step" => nested_sim.step_by(2).await?,
                                                s => if s == nested_event_.as_str() {
                                                    nested_sim.invoke("step").await?;
                                                    println!("nested_sim_{}: received custom event", n);
                                                },
                                            }
                                            let _ = s.send(Ok(Signal::new(rpc::behavior::Response::Empty, sig.ctx)));
                                        },
                                        rpc::behavior::Request::Shutdown => {
                                            println!("behavior: received shutdown");
                                            nested_sim.shutdown().await?;
                                            let _ = s.send(Ok(Signal::new(rpc::behavior::Response::Empty, sig.ctx)));
                                        }
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
                vec!["step".parse()?, nested_event.parse()?],
            )
            .await?;
        behaviors.push(behavior);
    }

    println!("behaviors spawned");

    // With some additional setup we can pass custom events to the nested sims
    // from the top level sim handle.
    //
    // In this case we're calling a custom event defined for only one of the
    // spawned behaviors. When the behavior receives the event it will itself
    // invoke a `step` event on the nested it's hoolding.
    sim.invoke("nested_event_0").await?;

    sim.step().await?;
    sim.step().await?;

    sim.shutdown().await?;

    Ok(())
}
