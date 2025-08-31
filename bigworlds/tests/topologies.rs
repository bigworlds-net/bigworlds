#![allow(warnings)]

use std::time::{Duration, Instant};

use itertools::Itertools;
use tokio::runtime;
use tokio_stream::StreamExt;

use bigworlds::{
    behavior::BehaviorTarget,
    leader, node,
    query::{Description, Filter, Map},
    rpc, server,
    sim::{self, SimConfig},
    worker, Caller, Context, Error, Executor, Model, Query, Signal,
};
use tokio_util::sync::CancellationToken;

#[allow(unused)]
mod common;

#[tokio::test]
async fn ring_random() -> anyhow::Result<()> {
    // Initialize logging.
    env_logger::init();

    let ring_size = 10;
    let (leader, workers) = common::ring_cluster(ring_size).await?;

    let entry_worker = rand::random_range(0..workers.len() - 1);
    println!("entry worker: {}", entry_worker);

    for (n, worker) in workers.iter().enumerate() {
        println!("worker {}: {}", n, worker.id().await?);
    }

    // Send a ctl request at a random worker node. The request will have to
    // travel all the way to the leader and back.
    let response = workers[entry_worker]
        .ctl
        .execute(Signal::new(
            rpc::worker::Request::ReplaceModel(common::model()).into_local(),
            Context::new(Caller::Unknown).delivery(Some(1)),
        ))
        .await??;

    // println!("ctx: {:?}", response.ctx);

    let leader_model: Model = leader
        .ctl
        .execute(Signal::from(rpc::leader::Request::GetModel.into_local()))
        .await??
        .discard_context()
        .try_into()?;
    assert_eq!(leader_model, common::model());

    // Response context should hold record of the exact number of network hops
    // that were made.
    // assert_eq!(response.ctx.worker_hop_count(), entry_worker + 1);
    assert!(
        response.ctx.worker_hop_count() == entry_worker + 1
            || response.ctx.worker_hop_count() == 10 - entry_worker
    );

    Ok(())
}

async fn ring_crossed_random() -> anyhow::Result<()> {
    let ring_size = 10;
    let (leader, workers) = common::ring_cluster(ring_size).await?;

    // Provide cross-ring connections.
    workers
        .get(ring_size / 3)
        .unwrap()
        .connect_to_local_leader(&leader)
        .await?;
    workers
        .get((ring_size / 3) * 2)
        .unwrap()
        .connect_to_local_leader(&leader)
        .await?;

    let entry_worker = rand::random_range(0..workers.len() - 1);

    // Send a ctl request at a random worker node. The request will have to
    // travel all the way to the leader and back.
    let response = workers[entry_worker]
        .ctl
        .execute(
            Signal::from(rpc::worker::Request::ReplaceModel(common::model()).into_local())
                .originating_at(rpc::Caller::Unknown),
        )
        .await??;

    let leader_model: Model = leader
        .ctl
        .execute(
            Signal::from(rpc::leader::Request::GetModel.into_local())
                .originating_at(rpc::Caller::Unknown),
        )
        .await??
        .discard_context()
        .try_into()?;
    assert_eq!(leader_model, common::model());

    let leader_model: Model = leader
        .ctl
        .execute(
            Signal::from(rpc::leader::Request::GetModel.into_local())
                .originating_at(rpc::Caller::Unknown),
        )
        .await??
        .discard_context()
        .try_into()?;
    assert_eq!(leader_model, common::model());

    // TODO: calculate hop count given random entry worker and provided
    // crossing edges.

    Ok(())
}
