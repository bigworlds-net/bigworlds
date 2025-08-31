//! Here we test executing custom `machine` logic programmatically.
//!
//! `machine` is a special kind of `behavior`. It allows executing a set of
//! predefined commands within a simple state machine regime.

use bigworlds::{machine, rpc, sim};

#[allow(unused)]
mod common;

#[tokio::test]
async fn machine_execute() -> anyhow::Result<()> {
    // Initialize logging.
    env_logger::init();

    // Spawn a local simulation instance.
    let mut sim = sim::spawn_with_config(common::sim_config_single_worker()).await?;

    // Spawn an empty machine.
    let machine = sim.spawn_machine(None, vec![]).await?;

    // Force-execute an arbitrary command on the machine.
    let response = machine.execute_cmd(machine::Command::NoOp).await?;
    assert_eq!(response, rpc::machine::Response::Empty);

    // Shut the machine down.
    machine.shutdown().await?;

    // This shouldn't run as we already sent the shutdown signal.
    let result = machine.execute_cmd(machine::Command::NoOp).await;
    assert!(result.is_err());

    Ok(())
}
