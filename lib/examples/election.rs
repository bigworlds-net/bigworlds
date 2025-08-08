//! Example showing off node-level supervision and leader election upon loosing
//! a leader.
//!
//! # Considerations
//!
//! Leader election is only interesting in the case of a cluster distributed
//! across multiple machines. For our purposes this means cluster participants
//! running on different `nodes`.
//!
//! Consider a situation where all cluster participants exist on the local
//! runtime. If we loose the leader task for whatever reason, the solution is
//! trivial: we spawn a new one in it's place, on the same runtime.
//!
//! On the other hand, in a situation where cluster participants are placed on
//! different nodes across multiple machines, there needs to be some additional
//! consideration when choosing the node to place the leader at.

#![allow(warnings)]

use std::time::Duration;

use tokio::runtime;
use tokio_stream::StreamExt;

use bigworlds::{
    behavior::BehaviorTarget,
    leader,
    node::{self, NodeConfig},
    query::{Description, Filter, Map},
    rpc,
    sim::{self, SimConfig},
    worker, Error, Executor, Model, Query, Signal,
};
use tokio_util::sync::CancellationToken;

mod common;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging.
    env_logger::init();

    // Set up multiple nodes across separate threads, each with it's own tokio
    // runtime. Each node spawns a worker listening at a known address.

    // Spawn node#0.
    std::thread::spawn(|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _enter = runtime.enter();
        let mut cancel = CancellationToken::new();

        runtime.block_on(async move {
            let mut node = node::spawn(NodeConfig::default(), cancel.clone()).unwrap();
            node.execute(rpc::node::Request::SpawnWorker(
                worker::Config {
                    listeners: vec!["quic://127.0.0.1:9910".parse().unwrap()],
                    ..Default::default()
                },
                None,
            ))
            .await
            .unwrap();
            println!("node#0 running");
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        })
    });

    // Spawn node#1.
    std::thread::spawn(|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _enter = runtime.enter();
        let mut cancel = CancellationToken::new();

        runtime.block_on(async move {
            let mut node = node::spawn(NodeConfig::default(), cancel.clone()).unwrap();
            node.execute(rpc::node::Request::SpawnWorker(
                worker::Config {
                    listeners: vec!["quic://127.0.0.1:9911".parse().unwrap()],
                    ..Default::default()
                },
                None,
            ))
            .await
            .unwrap();
            println!("node#1 running");
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        })
    });

    // Spawn a leader on the current runtime.
    let mut leader_cancel = CancellationToken::new();
    let mut leader = leader::spawn(Default::default(), leader_cancel.clone())?;
    println!("leader spawned");

    // Connect the leader to workers from the *remote* nodes.
    leader
        .connect_to_remote_worker("quic://127.0.0.1:9910".parse()?)
        .await?;
    println!("leader connected to remote worker 9910");
    leader
        .connect_to_remote_worker("quic://127.0.0.1:9911".parse()?)
        .await?;
    println!("leader connected to remote worker 9911");

    // Set the model on the leader.
    leader
        .execute(Signal::from(rpc::leader::Request::ReplaceModel(
            common::model(),
        )))
        .await?;

    // Step through a few times.
    for n in 0..2 {
        println!("stepping through");
        leader
            .execute(Signal::from(rpc::leader::Request::Step))
            .await?;
    }
    println!("done");

    // Kill the leader task.
    leader_cancel.cancel();

    // TODO: assert that the elections have started.

    // TODO: wait until elections are over and new leader is spawned on either
    // node.

    // TODO: connect as a client to confirm we're able to advance the
    // simulation.

    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    Ok(())
}
