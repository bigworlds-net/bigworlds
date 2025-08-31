//! Tests for the fs-backed entity archive.

#![allow(unused)]

use tokio_util::sync::CancellationToken;

use bigworlds::sim::SimConfig;
use bigworlds::time::Instant;
use bigworlds::{query, rpc, Executor, Query, Signal, SimHandle};

#[allow(unused)]
mod common;

const ENTITY_COUNT: usize = 1000;

#[tokio::test]
async fn entity_archive() -> anyhow::Result<()> {
    // Initialize logging.
    env_logger::init();

    let mut model = common::model();

    // Disable behaviors from the common model.
    model.behaviors.clear();
    // Don't include starting state entities so we get nice round numbers of
    // spawned entities.
    model.entities.clear();

    // TODO: fix for worker_count > 1. Spawning entities doesn't work as
    // expected with larger worker counts. This is likely a fault in the leader
    // entity spawning handler.
    let mut config = SimConfig {
        worker_count: 1,
        ..Default::default()
    };

    // Set the worker config up so that new entities are added directly to the
    // fs-backed archive instead of being instantiated into the main memory
    // storage.
    config.worker.partition.entity_create_to_archive = true;

    // Spawn a local sim instance.
    let mut sim =
        bigworlds::sim::spawn_from(model, None, config.clone(), CancellationToken::new()).await?;

    // Spawn the first batch of entities. These will be created as archived
    // right from the start.
    sim.spawn_entities(Some("cube".parse()?), ENTITY_COUNT)
        .await?;

    wait_until_entity_count_reached(&mut sim, ENTITY_COUNT).await?;

    // Spawn the second batch of entities to be instantiated in memory.
    //
    // Here we will also specify that we need the auto-archiver to put them
    // into the archive after a few seconds of inactivity.
    config.worker.partition.auto_archive_entities = true;
    config.worker.partition.archive_entity_after_secs = 3;
    config.worker.partition.entity_create_to_archive = false;
    sim.pull_config(config).await?;
    sim.spawn_entities(Some("cube".parse()?), ENTITY_COUNT)
        .await?;

    wait_until_entity_count_reached(&mut sim, ENTITY_COUNT * 2).await?;

    // Define a simple query.
    let query = Query::default().map(query::Map::All);

    // Issue a query ignoring the archive.
    let now = Instant::now();
    let product = sim
        .query(query.clone().archive(query::Archive::Ignore))
        .await?
        .to_vec();
    assert_eq!(product.len(), ENTITY_COUNT * 4);
    println!(
        "issued query [Archive::Ignore]: result len: {}, time: {}ms",
        product.len(),
        now.elapsed().as_millis()
    );

    // Issue a query including the archived entities. We should get a proper
    // result, meaning double the amount of returned entries.
    //
    // NOTE: we're including the archived entries but we're deciding not to
    // bring them up to the memory storage.
    //
    // NOTE: including archived entities for queries significantly increases
    // query processing time. In most cases it's best to use the option to
    // uplift entities selected with the query from archived state, such that
    // subsequent reads are faster.
    let now = Instant::now();
    let product = sim
        .query(query.clone().archive(query::Archive::Include))
        .await?
        .to_vec();
    assert_eq!(product.to_vec().len(), ENTITY_COUNT * 2 * 4);
    println!(
        "issued query [Archive::Include]: result len: {}, time: {}ms",
        product.len(),
        now.elapsed().as_millis()
    );

    // Finally, we shall wait for the auto-archiver to do its job.
    tokio::time::sleep(std::time::Duration::from_millis(5000)).await;

    // With the last query, again ignoring the archive, we should see an empty
    // product, as all the entities should've been archived by now.
    let now = Instant::now();
    let product = sim
        .query(
            query
                .archive(query::Archive::Ignore)
                .limits(Some(1000), None),
        )
        .await?
        .to_vec();
    assert_eq!(product.to_vec().len(), 0);
    println!(
        "after auto-archiver: issued query [Archive::Ignore]: result len: {}, time: {}ms",
        product.len(),
        now.elapsed().as_millis()
    );

    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    sim.shutdown().await?;

    Ok(())
}

async fn wait_until_entity_count_reached(sim: &mut SimHandle, count: usize) -> anyhow::Result<()> {
    // Probe the entity count until we have spawned all the entities.
    loop {
        let entities = sim.entities().await?;
        if entities.len() < count {
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
    Ok(())
}
