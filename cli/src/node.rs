use anyhow::Result;
use clap::ArgMatches;

use bigworlds::net::CompositeAddress;
use bigworlds::node::{self, NodeConfig};
use tokio_util::sync::CancellationToken;

pub fn cmd() -> clap::Command {
    use clap::{Arg, Command};

    Command::new("node")
        .about("Start a node")
        .display_order(24)
        .arg(
            Arg::new("config")
                .long("config")
                .help("Path to configuration file")
                .value_name("path"),
        )
        .arg(
            Arg::new("listeners")
                .long("listeners")
                .short('l')
                .help("List of listener addresses")
                .num_args(1..)
                .value_name("address"),
        )
        .arg(
            Arg::new("leader")
                .long("leader")
                .short('c')
                .help("Address of the cluster leader to contact")
                .value_name("address"),
        )
        .arg(
            Arg::new("server")
                .long("server")
                .short('s')
                .help("Establish a server at the level of the workplace")
                .num_args(1..)
                .value_name("address"),
        )
}

pub async fn start(matches: &ArgMatches, cancel: CancellationToken) -> Result<()> {
    // Extract the addresses to listen on.
    let listeners = matches
        .get_one::<String>("listeners")
        .map(|s| s.split(',').collect::<Vec<&str>>())
        .map(|v| {
            v.iter()
                .filter_map(|s| s.parse::<CompositeAddress>().ok())
                .collect()
        })
        .unwrap_or(vec![CompositeAddress::available()?]);

    let config = NodeConfig {
        listeners,
        ..Default::default()
    };

    node::spawn(config, cancel.clone())?;

    Ok(())
}
