//! Tests for shuffling of entities accross workers.

use tokio_util::sync::CancellationToken;

use bigworlds::{
    query::{Description, Filter, Map, Scope},
    rpc, sim, worker, Executor, Model, Query, Signal,
};

#[allow(unused)]
mod common;

/// For the sake of simplicity we define a situation where entities are
/// randomly reassigned to a worker on each simulation step.
#[tokio::test]
async fn random_shuffle() -> anyhow::Result<()> {
    // Initialize logging.
    env_logger::init();

    let cancel = CancellationToken::new();

    // Spawn a local sim instance. It will operate a single worker by itself.
    let mut sim = sim::spawn_with(common::sim_config_single_worker(), cancel.clone()).await?;

    // Manually spawn 2 additional workers and connect them to the leader.
    let foo_worker = worker::spawn(common::worker_config(), cancel.clone())?;
    foo_worker.connect_to_local_leader(&sim.leader).await?;
    let bar_worker = worker::spawn(common::worker_config(), cancel.clone())?;
    bar_worker.connect_to_local_leader(&sim.leader).await?;

    // Pull new model and propagate accross cluster.
    sim.pull_model(common::model()).await?;
    println!(
        "foo_worker[{}]: {:?}",
        foo_worker.id().await?,
        foo_worker.entities().await?
    );
    println!(
        "bar_worker[{}]: {:?}",
        bar_worker.id().await?,
        bar_worker.entities().await?
    );
    sim.initialize().await?;

    // Confirm the model was properly propagated.
    assert_eq!(foo_worker.model().await?, common::model());
    assert_eq!(bar_worker.model().await?, common::model());
    assert_eq!(
        TryInto::<Model>::try_into(
            foo_worker
                .ctl
                .execute(Signal::from(rpc::worker::Request::GetModel.into_local()))
                .await??
                .payload
        )?,
        common::model()
    );

    assert_eq!(
        sim.get_var("cube_0.position.float.x".parse()?)
            .await?
            .to_float(),
        0.
    );

    // Step through the simulation once and save initial entity state.
    sim.step().await?;

    // Confirm the lua behavior was properly executed on step.
    assert_eq!(
        sim.get_var("cube_0.position.float.x".parse()?)
            .await?
            .to_float(),
        1.
    );

    let _product = sim
        .query(
            Query::default()
                .scope(Scope::Global)
                .filter(Filter::Name(vec![
                    "cube_0".parse()?,
                    "cube_1".parse()?,
                    "cube_2".parse()?,
                ]))
                .map(Map::Component("color".parse()?))
                .description(Description::Entity),
        )
        .await?
        .to_named_map()?;

    println!("sim_worker: {:?}", sim.entities().await?);
    println!("foo_worker: {:?}", foo_worker.entities().await?);
    println!("bar_worker: {:?}", bar_worker.entities().await?);

    // Manually trigger reorganizing of entities across the cluster.
    sim.reorganize(true).await?;
    println!("sim: shuffled entities");

    // Confirm the behavior-mutated value was carried over when migrating the
    // entity.
    assert_eq!(
        sim.get_var("cube_0.position.float.x".parse()?)
            .await?
            .to_float(),
        1.
    );

    println!("sim_worker: {:?}", sim.entities().await?);
    println!("foo_worker: {:?}", foo_worker.entities().await?);
    println!("bar_worker: {:?}", bar_worker.entities().await?);

    // Step through again to confirm entity-bound behaviors were recreated
    // correctly on entity migration completion.
    sim.step().await?;

    // Confirm lua behavior was recreated and again executed on simulation
    // step.
    assert_eq!(
        sim.get_var("cube_0.position.float.x".parse()?)
            .await?
            .to_float(),
        2.
    );

    // NOTE: if we tried shuffling again immediately after, we might run into
    // the default per-entity cooldown, which is the configurable limit for how
    // often any one entity can be migrated between workers.
    //
    // sim.reorganize(true).await?;
    // println!("sim_worker: {:?}", sim.worker.entities().await?);
    // println!("foo_worker: {:?}", foo_worker.entities().await?);
    // println!("bar_worker: {:?}", bar_worker.entities().await?);

    sim.shutdown().await?;

    Ok(())
}
