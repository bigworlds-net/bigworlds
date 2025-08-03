use anyhow::Result;
use clap::ArgMatches;

use bigworlds::{leader, net::CompositeAddress};
use tokio_util::sync::CancellationToken;

pub fn cmd() -> clap::Command {
    use clap::{builder::PossibleValue, Arg, Command};

    Command::new("leader")
        .about("Start a cluster leader")
        .display_order(26)
        .arg(
            Arg::new("listeners")
                .long("listeners")
                .short('l')
                .help("List of listener addresses, delineated with a coma ','")
                .value_name("addresses"),
        )
        .arg(
            Arg::new("workers")
                .long("workers")
                .short('w')
                .help("Addresses of the workers to connect to, delineated with a coma ','")
                .value_name("addresses")
                .num_args(0..),
        )
        .arg(
            Arg::new("autostep")
                .long("autostep")
                .short('a')
                .help("Enable auto-stepping")
                .value_name("milliseconds"),
        )
}

/// Starts a leader task.
pub async fn start(matches: &ArgMatches, cancel: CancellationToken) -> Result<()> {
    let listeners = matches
        .get_one::<String>("listeners")
        .map(|s| s.split(',').collect::<Vec<&str>>())
        .map(|v| {
            v.iter()
                .filter_map(|s| s.parse::<CompositeAddress>().ok())
                .collect()
        })
        .unwrap_or(vec![CompositeAddress::available()?]);

    let autostep = matches.get_one::<String>("autostep").map(|a| {
        std::time::Duration::from_micros(
            a.parse()
                .expect("autostep value must be parsable into an integer"),
        )
    });

    let config = leader::Config {
        listeners,
        autostep,
        ..Default::default()
    };

    let mut handle = leader::spawn(config, cancel.clone())?;

    // Attempt connection to remote workers, if any.
    if let Some(worker_addrs) = matches.get_many::<String>("workers") {
        for addr in worker_addrs {
            if let Err(e) = handle.connect_to_remote_worker(addr.parse()?).await {
                error!("{e}");
            }
        }
    }

    Ok(())
}
