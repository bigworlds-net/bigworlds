//! Command-line application for running and interacting with `bigworlds`
//! systems.

use std::fs::{create_dir_all, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::{env, thread};

use anyhow::{Error, Result};
use bytes::BufMut;
use clap::builder::PossibleValue;
use clap::{value_parser, Arg, ArgAction, ArgMatches, Command};
use directories::ProjectDirs;
use fnv::FnvHashMap;
use notify::{RecommendedWatcher, Watcher};
use reqwest::header::AUTHORIZATION;
use reqwest::StatusCode;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use bigworlds::net;
use bigworlds::net::{CompositeAddress, Transport};
use bigworlds::util::get_snapshot_paths;
use bigworlds::SimHandle;
use bigworlds::{leader, server, worker};
use bigworlds::{rpc, Executor};

use crate::interactive;
use crate::interactive::{OnShutdown, OnShutdownAction};
use crate::tracing::LogLevel;
use crate::util::format_elements_list;

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const AUTHORS: &'static str = env!("CARGO_PKG_AUTHORS");

pub fn arg_matches() -> ArgMatches {
    let cmd = Command::new("bigworlds-cli")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .version(VERSION)
        .author(AUTHORS)
        .about("Simulate big worlds from the command line.")
        .arg(
            Arg::new("verbosity")
                .long("verbosity")
                .short('v')
                .display_order(100)
                .value_name("level")
                .default_value("info")
                .value_parser(["trace", "debug", "info", "warn", "error", "none"])
                .global(true)
                .help("Set the verbosity of the log output"),
        )
        .arg(Arg::new("threads").long("threads").global(true))
        .subcommand(crate::run::cmd())
        .subcommand(crate::server::cmd())
        .subcommand(crate::client::cmd())
        .subcommand(crate::worker::cmd())
        .subcommand(crate::leader::cmd())
        .subcommand(crate::node::cmd());

    cmd.get_matches()
}

/// Runs based on specified subcommand.
pub async fn start(matches: ArgMatches) -> Result<()> {
    init_logging(&matches);

    // Set up the mechanism for graceful shutdown
    let mut cancel = CancellationToken::new();
    let _cancel = cancel.clone();

    match matches.subcommand() {
        Some(("run", m)) => crate::run::start(m, _cancel).await?,
        Some(("client", m)) => crate::client::start(m, _cancel).await?,
        Some(("worker", m)) => crate::worker::start(m, _cancel).await?,
        Some(("leader", m)) => crate::leader::start(m, _cancel).await?,
        Some(("node", m)) => crate::node::start(m, _cancel).await?,
        _ => (),
    }

    // Return immediately if shutdown was already initiated, e.g. from the
    // interactive prompt.
    if cancel.is_cancelled() {
        // println!("Graceful shutdown detected...");
        tokio::time::sleep(Duration::from_secs(1)).await;
        return Ok(());
    }

    // Wait for either ctrl_c signal or cancellation trigger.
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Initiating graceful shutdown...");
            cancel.cancel();
        },
        _ = cancel.cancelled() => (),
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    Ok(())
}

/// Sets up logging based on settings from the matches.
fn init_logging(matches: &ArgMatches) -> Result<()> {
    let log_level = match matches.get_one::<String>("verbosity") {
        Some(s) => match s.as_str() {
            "0" | "none" => Some(LogLevel::Off),
            "1" | "err" | "error" | "min" => Some(LogLevel::Error),
            "2" | "warn" | "warning" | "default" => Some(LogLevel::Warn),
            "3" | "info" => Some(LogLevel::Info),
            "4" | "debug" => Some(LogLevel::Debug),
            "5" | "trace" | "max" | "all" => Some(LogLevel::Trace),
            _ => None,
        },
        _ => None,
    };
    crate::tracing::init(
        format!(
            "bigworlds-cli@{}",
            hostname::get().unwrap().to_string_lossy()
        ),
        log_level,
    )
}
