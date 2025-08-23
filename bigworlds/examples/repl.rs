//! This example shows how to quickly put together a simple interactive command
//! line interpreter using the building blocks provided by the library.

use std::io;
use std::io::Write;
use std::time::Duration;

use anyhow::Result;
use log::info;
use tokio_util::sync::CancellationToken;

use bigworlds::machine::{cmd, script, LocationInfo};
use bigworlds::sim::{self, SimConfig};

mod common {
    include!("../tests/common/mod.rs");
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging.
    let _ =
        simplelog::SimpleLogger::init(simplelog::LevelFilter::Info, simplelog::Config::default());

    let mut sim = match sim::spawn_from(
        common::model(),
        None,
        SimConfig::default(),
        CancellationToken::new(),
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            return Err(anyhow::anyhow!("failed spawning sim: {}", e));
        }
    };

    tokio::time::sleep(Duration::from_secs(1)).await;
    info!("entity count: {}", sim.entities().await?.len());

    // Spawn new machine behavior for executing commands.
    let machine = sim.spawn_machine(None, vec![]).await?;

    // Support amalgamating input using escape sequence `\\`.
    let mut input_amalg = String::new();

    // Main repl loop.
    'outer: loop {
        // Read user input.
        print!(">");
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("failed to read from stdin");

        if input.trim().ends_with("\\") {
            input_amalg.push_str(&input);
            continue;
        } else if !input_amalg.is_empty() {
            input_amalg.push_str(&input);
            input = input_amalg.clone();
            input_amalg = String::new();
        }

        // Parse input as instructions, potentially multiple lines.
        let instructions = match script::parser::parse_script(&input, LocationInfo::empty()) {
            Ok(i) => i,
            Err(e) => {
                println!("{:?}", e);
                continue;
            }
        };

        // Get only command prototypes.
        let mut cmd_protos = Vec::new();
        for instr in instructions {
            match instr.type_ {
                script::InstructionType::Command(cp) => cmd_protos.push(cp),
                _ => {
                    println!("not a command");
                    continue 'outer;
                }
            };
        }

        // Generate runnable commands.
        let mut commands = Vec::new();
        for (n, cmd_proto) in cmd_protos.iter().enumerate() {
            let mut location = LocationInfo::empty();
            location.line = Some(n);
            let command = match cmd::Command::from_prototype(&cmd_proto, &location, &cmd_protos) {
                Ok(c) => c,
                Err(e) => {
                    println!("{:?}", e);
                    continue 'outer;
                }
            };
            machine.execute_cmd(command.clone()).await.map(|_| ())?;
            commands.push(command);
        }

        // Execute commands on the machine behavior.
        machine.execute_cmd_batch(commands).await.map(|_| ())?;
    }
}
