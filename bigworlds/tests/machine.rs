//! This example showcases executing custom `machine` logic programmatically.
//!
//! `machine` is a special kind of `behavior`. It allows executing a set of
//! predefined commands within a simple state machine regime.

use bigworlds::{machine, rpc, sim};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Spawn a local simulation instance.
    let mut sim = sim::spawn().await?;

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
