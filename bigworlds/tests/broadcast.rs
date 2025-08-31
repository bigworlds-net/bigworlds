#![allow(unused)]

use std::time::Duration;

use bigworlds::{rpc, Caller, Executor, Participant, Signal};

#[allow(unused)]
mod common;

/// Broadcast a worker request to all workers on a star topology.
///
/// Expects traces of network hops registered in the resulting context.
///
/// # Process
///
/// The request will first be registered at the original worker. From there,
/// as the worker only has a direct connection to the leader, it will call
/// on the leader to broadcast the request to all the workers it is
/// connected to.
///
/// w0
/// |
/// l-------
/// |  |  |
/// w1 w2 w3
///
/// Workers receiving broadcasted request from the leader attempt to
/// broadcast further using the same mechanism. They inspect the signal
/// context and conclude it already went through the leader, so they skip
/// broadcasting through the leader.
///
/// Each worker sends a response to the leader, which subsequently sends
/// those responses to the original worker handler.
#[tokio::test]
async fn broadcast_star() -> anyhow::Result<()> {
    let cluster_size = 10;
    let (leader, workers) = common::star_cluster(cluster_size).await?;

    let request = rpc::worker::Request::Invoke {
        events: vec!["broadcasted_event".to_owned()],
        global: true,
    }
    .into_local();
    let worker = workers.first().unwrap();
    let response = worker.ctl.execute(Signal::from(request)).await??;

    // Looking at the response we should see all the workers registered on the
    // signal context just once.
    assert_eq!(response.ctx.worker_hop_count(), cluster_size);

    // The leader should have been registered only once.
    assert_eq!(response.ctx.leader_hop_count(), 1);

    Ok(())
}

#[tokio::test]
async fn broadcast_ring() -> anyhow::Result<()> {
    let cluster_size = 10;
    let (leader, workers) = common::ring_cluster(cluster_size).await?;

    for (n, worker) in workers.iter().enumerate() {
        println!("worker {}: {}", n, worker.id().await?);
    }

    let request = rpc::worker::Request::Invoke {
        events: vec!["broadcasted_event".to_owned()],
        global: true,
    }
    .into_local();
    let worker = workers.first().unwrap();
    let response = worker.ctl.execute(Signal::from(request)).await??;

    println!("{:?}", response.ctx);

    // Looking at the response we should see all the workers registered on the
    // signal context just once.
    assert_eq!(response.ctx.worker_hop_count(), cluster_size);

    // The leader should have been registered only once.
    assert_eq!(response.ctx.leader_hop_count(), 1);

    Ok(())
}
