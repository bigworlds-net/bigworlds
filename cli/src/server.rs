use std::time::Duration;

use anyhow::Result;
use bigworlds::server;
use clap::ArgMatches;

use bigworlds::leader;
use bigworlds::net::CompositeAddress;
use bigworlds::worker;
use tokio_util::sync::CancellationToken;

pub fn cmd() -> clap::Command {
    use clap::{builder::PossibleValue, Arg, Command};

    Command::new("server")
        .about("Start a server")
        .long_about(
            "Start a server. Server listens to incoming client connections \n\
            and fulfills client requests, anything from data transfers to entity spawning.\n\n\
            `server` subcommand allows for quickly starting either a local- or cluster-backed \n\
            server. Simulation can be started with either a path to scenario or a snapshot.\n\n\
            `bigworlds server -s ./scenarios/hello_world` \n    \
            (starts a server backed by local simulation process, based on a selected scenario)\n\n\
            NOTE: data sent between client and server is not encrypted, connection is not \n\
            secure! Basic authentication methods are provided, but they are more of \n\
            a convenience than a serious security measure.",
        )
        .display_order(21)
        .arg(Arg::new("path").value_name("path"))
        .arg(Arg::new("name").value_name("server_name"))
        .arg(Arg::new("description").value_name("server_description"))
        .arg(
            Arg::new("scenario")
                .long("scenario")
                .short('s')
                .display_order(1)
                .required(false)
                .value_name("scenario-path"),
        )
        .arg(
            Arg::new("snapshot")
                .long("snapshot")
                .display_order(2)
                .required(false)
                .value_name("snapshot-path"),
        )
        .arg(
            Arg::new("listeners")
                .long("listeners")
                .short('l')
                .help("List of listener addresses")
                .display_order(3)
                .num_args(1..)
                .value_delimiter(',')
                .required(false)
                .default_value("tcp://127.0.0.1:9123")
                .value_name("address"),
        )
        .arg(
            Arg::new("keep-alive")
                .long("keep-alive")
                .short('k')
                .display_order(4)
                .help(
                    "Server process will quit if it doesn't receive any messages within \
                the specified time frame (seconds)",
                )
                .value_name("seconds"),
        )
        .arg(
            Arg::new("client-keep-alive")
                .long("client-keep-alive")
                .help(
                    "Server process will remove client if it doesn't receive any messages \
                 from that client the specified time frame (seconds)",
                )
                .display_order(5)
                .value_name("seconds"),
        )
        .arg(
            Arg::new("compress")
                .long("compress")
                .short('x')
                .help("Use lz4 compression based on selected policy")
                .display_order(6)
                .value_name("compression-policy")
                .value_parser([
                    PossibleValue::new("all"),
                    PossibleValue::new("bigger_than_[n_bytes]"),
                ]),
        )
        .arg(
            Arg::new("leader")
                .long("leader")
                .short('o')
                .help("Start a server backed by a cluster leader")
                .display_order(100)
                .value_name("leader-address"),
        )
        .arg(
            Arg::new("cluster")
                .long("cluster")
                .short('c')
                .help("Start a server backed by an leader and a workplace")
                .display_order(101)
                .value_name("leader-address"),
        )
        .arg(
            Arg::new("workers")
                .long("workers")
                .short('w')
                .help(
                    "List of known cluster workers' addresses, only applicable if \
                `--leader`or `--cluster` option is also present",
                )
                .display_order(102)
                .value_name("worker-addresses"),
        )
        .arg(
            Arg::new("encodings")
                .long("encodings")
                .short('e')
                .help("List of supported encodings")
                .value_name("encodings-list"),
        )
        .arg(
            Arg::new("transports")
                .long("transports")
                .short('t')
                .help("List of supported transports")
                .value_name("transports-list"),
        )
}

/// Starts a server task.
pub async fn start(matches: &ArgMatches, cancel: CancellationToken) -> Result<()> {
    let listeners_addrs_str = matches
        .get_many("listeners")
        .expect("no listener addresses provided")
        .to_owned()
        .collect::<Vec<&String>>();

    let mut listeners = Vec::new();
    for listener_addr in listeners_addrs_str {
        let addr: CompositeAddress = listener_addr.parse()?;
        listeners.push(addr);
    }
    if listeners.is_empty() {
        panic!("no valid listener addresses provided");
    }

    if let Some(cluster_addr) = matches.get_one::<String>("cluster") {
        info!("listening for new workers on: {}", &cluster_addr);
    }

    let default = server::Config::default();
    println!("default transports list: {:?}", default.transports);
    let config = server::Config {
        listeners,
        name: match matches.get_one::<String>("name") {
            Some(n) => n.to_string(),
            None => "bigworlds_server".to_string(),
        },
        description: match matches.get_one::<String>("description") {
            Some(d) => d.to_string(),
            None => "It's a server alright.".to_string(),
        },
        self_keepalive: match matches.get_one::<String>("keep-alive") {
            Some(millis) => match millis.parse::<usize>() {
                Ok(ka) => match ka {
                    // 0 means keep alive forever
                    0 => None,
                    _ => Some(Duration::from_millis(ka as u64)),
                },
                Err(e) => panic!("failed parsing keep-alive (millis) value: {}", e),
            },
            // nothing means keep alive forever
            None => None,
        },
        poll_wait: Duration::from_millis(1),
        accept_delay: Duration::from_millis(100),

        client_keepalive: match matches.get_one::<u64>("client-keep-alive") {
            None => Some(Duration::from_secs(2)),
            Some(0) => None,
            Some(v) => Some(Duration::from_secs(*v)),
        },

        require_auth: false,
        use_compression: matches.get_one::<String>("compress").is_some(),
        auth_pairs: vec![],
        transports: match matches.get_one::<String>("transports") {
            Some(trans) => {
                println!("trans: {}", trans);
                let split = trans.split(',').collect::<Vec<&str>>();
                let mut transports = Vec::new();
                for transport_str in split {
                    if !transport_str.is_empty() {
                        transports.push(transport_str.parse()?);
                    }
                }
                transports
            }
            None => default.transports,
        },
        encodings: match matches.get_one::<String>("encodings") {
            Some(enc) => {
                let split = enc.split(',').collect::<Vec<&str>>();
                let mut encodings = Vec::new();
                for encoding_str in split {
                    if !encoding_str.is_empty() {
                        encodings.push(encoding_str.parse()?);
                    }
                }
                encodings
            }
            None => default.encodings,
        },
        ..Default::default()
    };

    // let worker_addrs = match matches.get_one::<String>("workers") {
    //     Some(wstr) => wstr
    //         .split(',')
    //         .map(|s| s.to_string())
    //         .collect::<Vec<String>>(),
    //     None => Vec::new(),
    // };

    // spawn the cluster leader
    let mut leader = leader::spawn(leader::Config::default(), cancel.clone())?;

    // spawn the worker
    let mut worker = worker::spawn(worker::Config::default(), cancel.clone())?;
    // initiate local connection to the leader
    leader.connect_to_local_worker(&worker, true).await?;

    // spawn server
    let mut server = bigworlds::server::spawn(config, worker.clone(), cancel.clone())?;
    server.connect_to_worker(&worker, true).await?;

    // // We can use server handle to send messages to the server, just as we
    // // would over the network
    // for n in 0..2 {
    //     // let mut executor = server.executor.clone();
    //     let server = server.clone();
    //     tokio::spawn(async move {
    //         println!("<< sending");
    //         let response = server
    //             .execute(
    //                 StatusRequest {
    //                     format: "".to_string(),
    //                 }
    //                 .into(),
    //             )
    //             .await
    //             .unwrap();
    //         println!(">> {:?}", response);
    //     });
    // }
    //
    // // TODO initialize services

    Ok(())
}
