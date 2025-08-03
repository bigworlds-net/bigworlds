use std::env;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Error, Result};
use bigworlds::sim::SimConfig;
use clap::builder::PossibleValue;
use clap::ArgMatches;

use bigworlds::rpc::msg::UploadProjectRequest;
use bigworlds::util::{find_model_root, get_snapshot_paths};
use bigworlds::Executor;
use bigworlds::{rpc, ServerConfig};
use notify::Watcher;
use tokio_util::sync::CancellationToken;

use crate::interactive;
use crate::util::format_elements_list;

pub fn cmd() -> clap::Command {
    use clap::{Arg, ArgAction, Command};

    Command::new("run")
        .about("Run a new world")
        .display_order(20)
        .long_about(
            "Start a new simulation run from a provided model.\n\
            If there are no arguments supplied the program will\n\
            look for the model in the current working directory.",
        )
        .arg(Arg::new("model").value_name("model"))
        .arg(
            Arg::new("scenario")
                .long("scenario")
                .num_args(0..)
                .default_missing_value("__any")
                .help("Provide name of a particular scenario to run"),
        )
        .arg(
            Arg::new("snapshot")
                .long("snapshot")
                .short('n')
                .help("Provide path to a snapshot file"),
        )
        .arg(
            Arg::new("server")
                .long("server")
                .short('s')
                .action(ArgAction::Append)
                .help("Expose a server, allowing for attaching clients and services")
                .value_name("server_address")
                .default_value("quic://127.0.0.1:0"),
        )
        .arg(
            Arg::new("leader")
                .long("leader")
                .short('l')
                .help("Expose the leader, allowing remote workers to join the cluster")
                .value_name("leader_address")
                .default_value("127.0.0.1:0"),
        )
        .arg(
            Arg::new("interactive")
                .action(ArgAction::SetTrue)
                .long("interactive")
                .short('i')
                .help("Go into an interactive prompt"),
        )
        .arg(
            Arg::new("autostep")
                .long("autostep")
                .short('a')
                .value_name("millis")
                .help("Set the autostep on the leader (milliseconds)"),
        )
        .arg(
            Arg::new("icfg")
                .long("icfg")
                .help("Provide path to interactive mode configuration file")
                .value_name("path")
                .default_value("./interactive.yaml"),
        )
        .arg(
            Arg::new("watch")
                .long("watch")
                .help("Watch project directory for changes")
                .value_name("on-change")
                .value_parser([PossibleValue::new("restart"), PossibleValue::new("update")]),
        )
}

/// Starts a new simulation run, using a model or a snapshot file.
///
/// # Resolving ambiguity
///
/// If an explicit option for loading either model or snapshot is not
/// provided, this function makes appropriate selection based on directory
/// structure, file name and file content analysis.
///
/// If the path argument is not provided, the current working directory is
/// used.
pub async fn start(matches: &ArgMatches, cancel: CancellationToken) -> Result<()> {
    let mut path = env::current_dir()?;
    match matches.get_one::<String>("model") {
        Some(p_str) => {
            let p = PathBuf::from(p_str);

            // if provided path is relative, append it to current working directory
            if p.is_relative() {
                path = path.join(p);
            }
            // otherwise if it's absolute then just set it as the path
            else {
                path = p;
            }
        }
        // choose what to do if no path was provided
        None => {
            let root = find_model_root(path.clone(), 4)?;

            // TODO show all runnable options to the user

            if let Some(snapshot) = matches.get_one::<String>("snapshot") {
                let available = get_snapshot_paths(root)?;
                if available.len() == 1 {
                    return start_run_snapshot(available[0].clone(), matches).await;
                } else if available.len() > 0 {
                    return Err(Error::msg(format!(
                        "choose one of the available snapshots: {}",
                        format_elements_list(&available)
                    )));
                } else {
                    return Err(Error::msg(format!("no snapshots available in project",)));
                }
            } else {
                //
            }
        }
    }

    path = path.canonicalize().unwrap_or(path);

    debug!("path: {:?}", path);

    if matches.get_one::<String>("scenario").is_none() {
        return start_run_model(path, matches, cancel).await;
    } else if matches.get_one::<String>("snapshot").is_none() {
        return start_run_snapshot(path, matches).await;
    } else {
        if path.is_file() {
            // decide whether the path looks more like scenario or snapshot
            if let Some(ext) = path.extension() {
                if ext == "toml" {
                    return start_run_model(path, matches, cancel).await;
                }
            }
            return start_run_snapshot(path, matches).await;
        }
        // path is provided but it's a directory
        else {
            let root = find_model_root(path.clone(), 4)?;

            // TODO allow choosing from available scenarios with additional
            // flag

            return start_run_model(root, matches, cancel).await;
        }
    }

    Ok(())
}

async fn start_run_model(
    model_path: PathBuf,
    matches: &ArgMatches,
    cancel: CancellationToken,
) -> Result<()> {
    let model_root = find_model_root(model_path.clone(), 4)?;

    info!(
        "Spawning new world from model at: {}",
        model_root.to_string_lossy()
    );

    let mut config = SimConfig::default();
    if let Some(leader) = matches.get_one::<String>("leader") {
        config.leader.listeners.push(leader.parse()?);
    }
    if let Some(autostep_millis) = matches.get_one::<String>("autostep") {
        config.leader.autostep = Some(Duration::from_millis(autostep_millis.parse()?));
    }
    if let Some(server) = matches.get_many::<String>("server") {
        config.server = Some(ServerConfig {
            listeners: server
                .into_iter()
                .filter_map(|s| {
                    s.parse()
                        .inspect_err(|e| warn!("provided incorrect address: {s}"))
                        .ok()
                })
                .collect::<Vec<_>>(),
            ..Default::default()
        })
    }

    let mut sim = bigworlds::sim::spawn_from_path(
        model_root,
        matches.get_one::<String>("scenario").map(|s| s.as_str()),
        config,
        cancel.clone(),
    )
    .await?;

    tokio::time::sleep(Duration::from_millis(300)).await;

    if matches.get_flag("interactive") {
        info!("Starting an interactive prompt");

        let config_path = matches
            .get_one::<String>("icfg")
            .unwrap_or(&interactive::config::CONFIG_FILE.to_string())
            .to_string();

        let mut on_change = None;

        // store watcher here so it doesn't go out of scope
        let mut watcher: Option<notify::RecommendedWatcher> = None;

        if matches.get_one::<String>("watch").is_some() {
            use std::sync::Mutex;
            let watch_path = find_model_root(model_path.clone(), 4)?;
            info!(
                "watching changes at project path: {}",
                watch_path.to_string_lossy()
            );
            let change_detected = std::sync::Arc::new(Mutex::new(false));
            let change_detected_clone = change_detected.clone();
            let mut _watcher: notify::RecommendedWatcher =
                notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                    match res {
                        Ok(event) => {
                            // Disregard changes in build directories of cargo projects
                            // nested inside modules.
                            // TODO implement better filtering than basic word matching
                            if !event
                                .paths
                                .iter()
                                .any(|p| p.to_str().unwrap().contains("target"))
                            {
                                debug!("change detected: {:?}", event);
                                *change_detected_clone.lock().unwrap() = true;
                            } else {
                                trace!(
                                    "change detected, but ignored\
                                    (build directory?), paths: {:?}",
                                    event.paths
                                )
                            }
                        }
                        Err(e) => {
                            error!("watch error: {:?}", e);
                            *change_detected_clone.lock().unwrap() = true;
                        }
                    }
                })?;
            _watcher.watch(&watch_path, notify::RecursiveMode::Recursive)?;

            watcher = Some(_watcher);

            on_change = match matches.get_one::<String>("watch").map(|s| s.as_str()) {
                Some("restart") | None => Some(interactive::OnChange {
                    trigger: change_detected.clone(),
                    action: interactive::OnChangeAction::Restart,
                }),
                Some("update") => Some(interactive::OnChange {
                    trigger: change_detected.clone(),
                    action: interactive::OnChangeAction::UpdateModel,
                }),
                Some(action) => {
                    return Err(Error::msg(format!(
                        "not recognized change action for `watch`: {}",
                        action,
                    )))
                }
            };
        }

        interactive::start(sim.as_client(), &config_path, on_change, cancel).await?;
    }
    Ok(())
}

async fn start_run_snapshot(path: PathBuf, matches: &ArgMatches) -> Result<()> {
    info!("Running interactive session using snapshot at: {:?}", path);

    // start the local sim task
    let mut sim = bigworlds::sim::spawn().await?;

    // let response = sim
    //     .server
    //     .execute(
    //         UploadProjectRequest {
    //             archive: Vec::new(),
    //         }
    //         .into(),
    //     )
    //     .await?;

    let cancel = sim.cancel.clone();
    if matches.get_flag("interactive") {
        interactive::start(
            sim.as_client(),
            matches
                .get_one::<String>("icfg")
                .unwrap_or(&interactive::config::CONFIG_FILE.to_string()),
            None,
            cancel,
        );
    }
    Ok(())
}
