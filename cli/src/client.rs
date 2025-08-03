use std::str::FromStr;
use std::time::Duration;

use anyhow::{Error, Result};
use bigworlds::client::r#async::AsyncClient;
use clap::ArgMatches;

use bigworlds::client::{self, CompressionPolicy, WsClient};
use bigworlds::net::{CompositeAddress, Transport};
use tokio_util::sync::CancellationToken;

use crate::interactive;

pub fn cmd() -> clap::Command {
    use clap::{Arg, ArgAction, Command};

    Command::new("client")
        .about("Start an interactive client session")
        .long_about(
            "Start an interactive client session.\n\n\
            Establishes a client connection to a server at specified address,\n\
            and provides a REPL-style interface for interacting with that\n\
            server.",
        )
        .display_order(25)
        .arg(
            Arg::new("server-addr")
                .long("server")
                .short('s')
                .help("Address of the server")
                .required(true)
                .value_name("address"),
        )
        .arg(
            Arg::new("client-addr")
                .long("address")
                .help("Address of this client")
                .value_name("address"),
        )
        .arg(Arg::new("auth").long("auth").short('a').help(
            "Authentication pair used when connecting to server \
                [example value: user,password]",
        ))
        .arg(
            Arg::new("icfg")
                .long("icfg")
                .help("Path to interactive config file")
                .value_name("path")
                .default_value("./interactive.yaml"),
        )
        .arg(
            Arg::new("name")
                .long("name")
                .short('n')
                .help("Name for the client")
                .value_name("string"),
        )
        .arg(
            Arg::new("blocking")
                .action(ArgAction::SetTrue)
                .long("blocking")
                .short('b')
                .help(
                    "Sets the client as blocking, requiring it to explicitly \
                agree to step simulation forward",
                ),
        )
        .arg(
            Arg::new("compress")
                .long("compress")
                .short('c')
                .help(
                    "Sets whether outgoing messages should be compressed, \
                and based on what policy [possible values: all, bigger_than_n, data, none]",
                )
                .value_name("policy")
                .default_value("none"),
        )
        .arg(
            Arg::new("heartbeat")
                .long("heartbeat")
                .help("Set the heartbeat frequency in heartbeat per n seconds")
                .value_name("secs")
                .default_value("1"),
        )
        .arg(
            Arg::new("encodings")
                .long("encodings")
                .short('e')
                .help("Supported encodings that can be used when talking to server")
                .default_value("bincode"),
        )
        .arg(
            Arg::new("transports")
                .long("transports")
                .short('t')
                .help("Supported transports that can be used when talking to server")
                .default_value("tcp"),
        )
}

pub async fn start(matches: &ArgMatches, cancel: CancellationToken) -> Result<()> {
    let addr = matches
        .get_one::<String>("server-addr")
        .map(|s| s.to_string())
        .ok_or(Error::msg("server adddress must be provided"))?;
    let addr = CompositeAddress::from_str(&addr)?;
    let config = client::Config {
        name: matches
            .get_one::<String>("name")
            .unwrap_or(&"cli-client".to_string())
            .to_string(),
        heartbeat_interval: match matches.get_one::<String>("heartbeat") {
            Some(h) => Some(Duration::from_secs(h.parse()?)),
            None => None,
        },
        is_blocking: matches.get_flag("blocking"),
        compress: CompressionPolicy::from_str(matches.get_one::<String>("compress").unwrap())?,
        // encodings: match matches.value_of("encodings") {
        //     Some(encodings_str) => {
        //         let split = encodings_str.split(',').collect::<Vec<&str>>();
        //         let mut transports = Vec::new();
        //         for transport_str in split {
        //             if !transport_str.is_empty() {
        //                 transports.push(transport_str.parse()?);
        //             }
        //         }
        //         transports
        //     }
        //     None => Vec::new(),
        // },
        // transports: match matches.value_of("transports") {
        //     Some(transports_str) => {
        //         let split = transports_str.split(',').collect::<Vec<&str>>();
        //         let mut transports = Vec::new();
        //         for transport_str in split {
        //             if !transport_str.is_empty() {
        //                 transports.push(transport_str.parse()?);
        //             }
        //         }
        //         transports
        //     }
        //     None => Vec::new(),
        // },
    };
    match addr.transport {
        Some(Transport::WebSocket) | Some(Transport::SecureWebSocket) => {
            let client = WsClient::connect(addr, config).await?;
            // let client = WsClient::connect_direct().await?;
            interactive::start(
                client,
                matches
                    .get_one::<String>("icfg")
                    .unwrap_or(&interactive::config::CONFIG_FILE.to_string()),
                None,
                cancel.clone(),
            )
            .await;
        }
        Some(Transport::Quic) | None => {
            let client = bigworlds::client::Client::connect(addr, config).await?;
            interactive::start(
                client,
                matches
                    .get_one::<String>("icfg")
                    .unwrap_or(&interactive::config::CONFIG_FILE.to_string()),
                None,
                cancel.clone(),
            )
            .await;
        }
        _ => unimplemented!(),
    }

    Ok(())
}
