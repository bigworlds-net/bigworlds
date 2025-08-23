//! Example showing off a cluster setup following a ring topology.
//!
//! # Notes
//!
//! Since the leader is required for handling certain operations, putting it
//! on a ring and having workers access it via other workers' proxying the
//! request, can be quite inefficient.
//!
//! Changing up the classic ring topology a little and introducing one or more
//! cross-ring connections may be desirable.
//!
//! # Diagram
//!
//! ```
//!      #l
//!   #w6 \ #w0
//! #w5  / \  #w1
//!   #w4   #w2
//!      #w3s
//!
//! # == participant on the ring
//! l == leader
//! w == worker
//! s == server
//! \ == optional cross-ring connections
//!
//! ```

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
    worker, Error, Executor, Model, Query, Signal,
};
use tokio_util::sync::CancellationToken;

mod common;

/// Here the size denotes the number of workers in the cluster.
const DEFAULT_RING_SIZE: usize = 7;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging.
    env_logger::init();

    // Read ring size from the command line.
    let ring_size = std::env::args()
        .skip(1)
        .next()
        .map(|s| s.parse().expect("argument should be parsable as usize"))
        .unwrap_or(DEFAULT_RING_SIZE);

    let cross_connection = false;

    let mut cancel = CancellationToken::new();

    // Spawn leader.
    let mut leader = leader::spawn(Default::default(), cancel.clone())?;

    // Provide worker config with disabled archive for faster startup times.
    let mut worker_config = worker::Config::default();
    worker_config.partition.enable_archive = false;

    // Spawn workers.
    let mut workers = vec![];
    for n in 0..ring_size {
        let worker = worker::spawn(worker_config.clone(), cancel.clone())?;
        workers.push(worker);
    }

    // Connect all workers iterating forward.
    for (a, b) in workers.iter().tuple_windows() {
        a.connect_to_local_worker(b).await?;
    }

    // Close the ring by connecting the leader to first and last worker.
    leader
        .connect_to_local_worker(workers.first().unwrap(), true)
        .await?;
    leader
        .connect_to_local_worker(workers.last().unwrap(), true)
        .await?;

    // Optionally we can provide cross-ring connections.
    if cross_connection {
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
    }

    // Establish a server with one of the workers.
    // let server = server::spawn(Default::default(), cancel.clone())?;
    // server.connect_to_worker(&worker_3, true).await?;

    println!("ring cluster established");

    let mid_worker = ring_size / 2;

    let now = Instant::now();

    // Send a request to the worker that will need to be passed to the leader
    // through the networked workers.
    let response = workers[mid_worker]
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

    // Response context should hold record of the exact number of network hops
    // that were made.
    let context = response.ctx.unwrap();
    if !cross_connection {
        assert_eq!(context.worker_hop_count(), (mid_worker + 1) * 2);
    }
    // We should also be able to use the context to calculate the time our
    // signal spent inside the network. It should be somewhat close to our
    // local measurement.
    let ctx_transfer_time_ms = context.total_transfer_time_micros();
    println!(
        "got response, hop count: {}, time elapsed (context): {}micros, time elapsed (local): {}micros",
        context.worker_hop_count(),
        ctx_transfer_time_ms,
        now.elapsed().as_micros()
    );
    if context.hops.len() < 100 {
        println!("hops: {:#?}", context.hops);
    }

    // TODO: perform an equivalent call through the exposed server.
    // server.execute(rpc::msg::Message::pul)

    cancel.cancel();
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    Ok(())
}
