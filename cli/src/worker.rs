use anyhow::Result;
use clap::ArgMatches;

use bigworlds::{leader, net::CompositeAddress, rpc, server, worker, Executor};
use tokio_util::sync::CancellationToken;

pub fn cmd() -> clap::Command {
    use clap::{Arg, Command};

    Command::new("worker")
        .about("Start a worker")
        .long_about(
            "Start a worker. Worker is the smallest independent part\n\
            of a system where a collection of worker nodes collaboratively\n\
            simulate a larger world.\n\n\
            Worker must have a connection to the main leader, whether direct\n\
            or indirect. Indirect connection to leader can happen through another\n\
            worker or a relay.",
        )
        .display_order(23)
        .arg(
            Arg::new("address")
                .long("address")
                .short('a')
                .help("Set the listener address(es) for the worker")
                .value_name("address"),
        )
        .arg(
            Arg::new("remote-worker")
                .long("remote-worker")
                .help("Address of a remote worker to connect to")
                .value_name("address"),
        )
        .arg(
            Arg::new("remote-leader")
                .long("remote-leader")
                .help("Address of the cluster leader to connect to")
                .value_name("address"),
        )
        .arg(
            Arg::new("leader")
                .long("leader")
                .short('l')
                .help("Establish a leader on the same process")
                .num_args(0..)
                .value_name("address"),
        )
        .arg(
            Arg::new("server")
                .long("server")
                .short('s')
                .help("Establish a server backed by this worker")
                .num_args(0..)
                .value_name("address"),
        )
        .arg(
            Arg::new("max_ram")
                .long("max_ram")
                .help("Maximum allowed memory usage")
                .value_name("megabytes"),
        )
        .arg(
            Arg::new("max_disk")
                .long("max_disk")
                .help("Maximum allowed disk usage")
                .value_name("megabytes"),
        )
        .arg(
            Arg::new("max_transfer")
                .long("max_transfer")
                .help("Maximum allowed network transfer usage")
                .value_name("megabytes"),
        )
}

pub async fn start(matches: &ArgMatches, cancel: CancellationToken) -> Result<()> {
    // Extract the addresses to listen on.
    let listeners = matches
        .get_one::<String>("address")
        .map(|s| s.split(',').collect::<Vec<&str>>())
        .map(|v| {
            v.iter()
                .filter_map(|s| s.parse::<CompositeAddress>().ok())
                .collect()
        })
        .unwrap_or(vec![CompositeAddress::available()?]);

    // Create worker configuration.
    let config = worker::Config {
        listeners,
        addr: matches.get_one::<String>("address").cloned(),
        max_ram_mb: matches.get_one::<usize>("max_ram").unwrap_or(&0).to_owned(),
        max_disk_mb: matches
            .get_one::<usize>("max_disk")
            .unwrap_or(&0)
            .to_owned(),
        max_transfer_mb: matches
            .get_one::<usize>("max_transfer")
            .unwrap_or(&0)
            .to_owned(),
        ..Default::default()
    };

    // Spawn worker task, returning a handle with executors.
    let mut worker = worker::spawn(config, cancel.clone())?;

    // If leader flag is present spawn a leader task.
    if let Some(listeners) = matches.get_many::<String>("leader") {
        let leader = leader::spawn(
            leader::Config {
                listeners: listeners
                    .into_iter()
                    .filter_map(|l| l.parse().ok())
                    .collect::<Vec<_>>(),
                autostep: Some(std::time::Duration::from_millis(1000)),
                ..Default::default()
            },
            cancel.clone(),
        )?;
        worker.connect_to_local_leader(&leader).await?;
    }

    // If server flag is present spawn a server task.
    if let Some(listeners) = matches.get_many::<String>("server") {
        let server = server::spawn(
            server::Config {
                listeners: listeners
                    .into_iter()
                    .filter_map(|l| l.parse().ok())
                    .collect::<Vec<_>>(),
                ..Default::default()
            },
            worker.clone(),
            cancel.clone(),
        )?;
        worker.connect_to_local_server(&server).await?;
    }

    // If worker addresses are specified, attempt to connect.
    if let Some(worker_addrs) = matches.get_many::<String>("remote-worker") {
        for addr in worker_addrs {
            // let addr = addr.parse()?;
            if let Err(e) = worker.connect_to_worker(addr).await {
                error!("{e}");
            }
        }
    }

    // If leader address is specified, attempt a connection.
    if let Some(leader_addr) = matches.get_one::<String>("remote-leader") {
        let leader_addr = leader_addr.parse()?;
        if let Err(e) = worker
            .ctl
            .execute(bigworlds::Signal::new(
                rpc::worker::Request::ConnectToLeader(leader_addr).into(),
                None,
            ))
            .await
        {
            error!("{e}");
        }
    }

    Ok(())
}
