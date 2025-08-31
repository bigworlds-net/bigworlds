//! Most basic stress-test, spawning a million entities into memory.

use bigworlds::time::Instant;
use bigworlds::{query, rpc, Executor, Query, Signal};

#[allow(unused)]
mod common {
    include!("../tests/common/mod.rs");
}

// NOTE: one million entities will take up at least 4GBs of RAM.
const TARGET_ENTITY_COUNT: usize = 1000000;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging.
    env_logger::init();

    let mut model = common::model();

    // Lua behaviors are incredibly memory-hungry, comparatively speaking.
    // Let's disable them for now.
    model.behaviors.clear();

    // Spawn a local sim instance.
    let mut sim = bigworlds::sim::spawn_from_model(model).await?;

    // Spawn the desired number of entities, all based on the same prefab.
    // NOTE: by default all newly spawned entities will get instantiated in
    // memory. See `examples/archive.rs` for an example of spawning entities
    // straight to fs-backed archived state.
    sim.spawn_entities(Some("cube".parse()?), TARGET_ENTITY_COUNT)
        .await?;

    // Define a simple query filtering on entity name. Time taken by this query
    // should scale linearly with number of entities, at least in theory.
    let query = Query::default()
        .filter(query::Filter::Name(vec!["cube_0".parse().unwrap()]))
        .map(query::Map::All);

    // Spawn an unsynced behavior task.
    //
    // This behavior will not block simulation step processing as a synced
    // behavior would. In this case it means we will get more neatly separated
    // timings for our query and the performed simulation steps.
    let _ = sim
        .spawn_behavior_unsynced(|mut stream, exec| {
            Box::pin(async move {
                loop {
                    match stream.recv().await {
                        Ok(req) => match req {
                            rpc::behavior::Request::Event(event) => match event.as_str() {
                                "step" => {
                                    let now = Instant::now();
                                    let resp = exec
                                        .execute(Signal::new(
                                            rpc::worker::Request::ProcessQuery(query.clone()),
                                            rpc::Context::new(rpc::Caller::Behavior),
                                        ))
                                        .await??;
                                    println!(
                                        "behavior performed query in {}ms, response: {:?}",
                                        now.elapsed().as_millis(),
                                        resp,
                                    );
                                }
                                _ => (),
                            },
                            rpc::behavior::Request::Shutdown => {
                                println!("behavior: received shutdown");
                                break;
                            }
                        },
                        Err(e) => log::warn!("err: {:?}", e),
                    }
                }
                println!("errored");

                Ok(())
            })
        })
        .await?;

    // Probe the entity count until we have spawned all the entities.
    loop {
        let entities = sim.entities().await?;
        if entities.len() < TARGET_ENTITY_COUNT {
            println!("entity count: {}", entities.len());
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        } else {
            let sys = sysinfo::System::new_all();
            let process = sys.process(sysinfo::get_current_pid().unwrap()).unwrap();

            println!(
                "reached target entity count: {}, current process memory: {}MB (virtual: {}MB)",
                sim.entities().await?.len(),
                process.memory() / 1000000,
                process.virtual_memory() / 1000000
            );
            break;
        }
    }

    // Step through a few times, measuring and displaying time elapsed for each
    // step.
    for _ in 0..3 {
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        let now = Instant::now();
        sim.step().await?;
        println!("step: {}ms", now.elapsed().as_millis());
    }

    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    sim.shutdown().await?;

    Ok(())
}
