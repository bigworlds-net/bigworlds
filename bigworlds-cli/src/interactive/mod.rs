//! Defines an interactive interface for the command line.

mod compl;
pub mod config;

mod img_print;

use std::io;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;

use anyhow::Result;
use linefeed::inputrc::parse_text;
use linefeed::Signal;
use linefeed::{Interface, ReadResult};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use bigworlds::client::r#async::AsyncClient;
use bigworlds::query;

use crate::interactive::config::{Config, CONFIG_FILE};

use compl::MainCompleter;

pub struct OnChange {
    pub trigger: Arc<Mutex<bool>>,
    pub action: OnChangeAction,
}

pub enum OnChangeAction {
    Restart,
    UpdateModel,
}

pub struct OnShutdown {
    pub trigger: CancellationToken,
    pub action: OnShutdownAction,
}

pub enum OnShutdownAction {
    Custom,
    Quit,
}

/// Variant without the external change trigger.
pub async fn start_simple(
    client: impl AsyncClient,
    config_path: &str,
    cancel: CancellationToken,
) -> Result<()> {
    start(client, config_path, None, cancel).await
}

/// Entry point for the interactive interface.
///
/// # Introducing runtime changes
///
/// Interactive interface supports updating simulation model or outright
/// restarting the simulation using an external trigger. This is used for
/// supporting a "watch" mode where changes to project files result in
/// triggering actions such as restarting the simulation using newly
/// introduced changes.
pub async fn start(
    mut client: impl AsyncClient,
    config_path: &str,
    on_change: Option<OnChange>,
    mut cancel: CancellationToken,
) -> Result<()> {
    'outer: loop {
        debug!("interactive: start outer");

        // Check if there was a change triggered.
        if let Some(ocm) = &on_change {
            let mut oc = ocm.trigger.lock().unwrap();
            if *oc == true {
                // Switch off the trigger.
                *oc = false;

                match ocm.action {
                    OnChangeAction::Restart => {
                        // Send request to restart.
                        todo!()
                    }
                    _ => (),
                }

                continue;
            }
        }

        let interface = Arc::new(Interface::new("interactive")?);

        // TODO: completer is currently gutted.
        interface.set_completer(Arc::new(MainCompleter {
            // driver: driver_arc.clone(),
        }));

        // Try loading config from file, else get a new default one.
        let mut config = match Config::new_from_file(config_path) {
            Err(e) => {
                if e.kind() == io::ErrorKind::NotFound {
                    info!(
                        "Config file {} doesn't exist, loading default config settings",
                        CONFIG_FILE
                    );
                    Config::new()
                } else {
                    warn!("There was a problem parsing the config file, loading default config settings ({})", e);
                    Config::new()
                }
            }
            Ok(mut c) => {
                info!("Loading config settings from file (found {})", CONFIG_FILE);
                if c.turn_steps == 0 {
                    c.turn_steps = 1;
                }
                c
            }
        };

        interface.set_prompt(&create_prompt(&mut client, &config).await?)?;

        if cancel.is_cancelled() {
            break 'outer;
        }

        let mut do_run = false;
        let mut do_run_loop = false;
        let mut do_run_freq = None;
        let mut last_time_insta = std::time::Instant::now();
        let mut just_left_loop = false;

        let mut run_loop_count = 0;

        interface.set_report_signal(Signal::Interrupt, true);
        interface.set_report_signal(Signal::Break, true);
        interface.set_report_signal(Signal::Quit, true);

        println!("\nYou're now in interactive mode.");
        println!("See possible commands with \"help\". Exit using \"quit\" or ctrl-d.");

        // Start main loop.
        loop {
            if cancel.is_cancelled() {
                break 'outer;
            }

            // Check if the client connection is still alive.
            if let Err(_) = client.is_alive().await {
                error!("Server terminated the connection...");
                break 'outer;
            };

            if do_run_loop || do_run_freq.is_some() {
                if let Some(hz) = do_run_freq {
                    let target_delta_time = Duration::from_secs(1) / hz;
                    let time_now = std::time::Instant::now();
                    let delta_time = time_now - last_time_insta;
                    if delta_time >= target_delta_time {
                        last_time_insta = time_now;
                        do_run = true;
                    } else {
                        do_run = false;
                    }
                }

                if do_run_loop {
                    if run_loop_count <= 0 {
                        do_run = false;
                        do_run_loop = false;
                        run_loop_count = 0;
                    } else {
                        run_loop_count -= 1;
                        do_run = true;
                    }
                }

                if do_run {
                    client.step(1).await?;
                    interface.set_prompt(&create_prompt(&mut client, &config).await?)?;

                    let read_result = match interface.read_line_step(Some(Duration::from_micros(1)))
                    {
                        Ok(res) => match res {
                            Some(r) => r,
                            None => continue,
                        },
                        Err(e) => continue,
                    };
                    match read_result {
                        _ => {
                            do_run = false;
                            do_run_freq = None;
                            do_run_loop = false;
                            run_loop_count = 0;
                            continue;
                        }
                    }
                }

                continue;
            }

            if let Some(res) = interface.read_line_step(Some(Duration::from_millis(10)))? {
                match res {
                    ReadResult::Input(line) => {
                        if !line.trim().is_empty() {
                            interface.add_history_unique(line.clone());
                        }

                        let (cmd, args) = split_first_word(&line);
                        match cmd {
                            "" => {
                                if let Err(bigworlds::Error::ErrorResponse(e)) =
                                    client.step(1).await
                                {
                                    if e.contains("connection closed") {
                                        error!("Server terminated the connection before step was processed...");
                                        cancel.cancel();
                                    } else {
                                        warn!("Couldn't process step: {}", e);
                                    }
                                }
                            }
                            "invoke" => {
                                client.invoke(args).await?;
                            }
                            "run" => {
                                do_run_loop = true;
                                run_loop_count = args.parse::<i32>().unwrap();
                            }
                            "run-until" => {
                                unimplemented!();
                            }
                            "runf" => {
                                interface.lock_reader();
                                let mut loop_count = args.parse::<u32>().unwrap();
                                while loop_count > 0 {
                                    client.step(1).await?;
                                    loop_count -= 1;
                                }
                                interface.set_prompt(
                                    &create_prompt(&mut client, &config).await?.as_str(),
                                )?;
                            }
                            "runf-until" => {
                                unimplemented!();
                            }
                            "run-freq" => {
                                let hz = args.parse::<usize>().unwrap_or(10);
                                do_run_freq = Some(hz as u32);
                            }
                            "runf-hz" => {
                                let hz = args.parse::<usize>().unwrap_or(10);
                                let mut last = Instant::now();
                                loop {
                                    if Instant::now() - last
                                        < Duration::from_millis(1000 / hz as u64)
                                    {
                                        std::thread::sleep(Duration::from_millis(1));
                                        continue;
                                    }
                                    last = Instant::now();
                                    client.step(1).await?;
                                    interface.set_prompt(
                                        create_prompt(&mut client, &config).await?.as_str(),
                                    )?;
                                }
                            }
                            // List variables.
                            // TODO: make this more useful by supporting basic
                            // globbing and such.
                            "ls" | "l" => {
                                let mut query = bigworlds::Query::default()
                                    // Explicitly request entities from all workers
                                    .scope(query::Scope::Global)
                                    .map(query::Map::All)
                                    .description(query::Description::Addressed);

                                // HACK: filter output by single entity name
                                if !args.is_empty() {
                                    println!("filtering by name: {args}");
                                    query =
                                        query.filter(query::Filter::Name(vec![args.to_owned()]));
                                }

                                println!("executing query: {:?}", query);
                                let product = client.query(query).await?;

                                for (addr, var) in product.to_map()? {
                                    println!("{addr} = {var:?}");
                                }
                            }
                            // Despawn entity.
                            "despawn" => {
                                client
                                    .despawn_entity(args)
                                    .await
                                    .inspect_err(|e| error!("{}", e));
                            }
                            // Spawn entity.
                            "spawn" => {
                                let split = args.split(" ").collect::<Vec<&str>>();
                                client
                                    .spawn_entity(split[0].to_owned(), Some(split[1].to_owned()))
                                    .await
                                    .inspect_err(|e| error!("{}", e));
                            }
                            // Spawn n entities.
                            "nspawn" => {
                                let split = args.split(" ").collect::<Vec<&str>>();
                                for n in 0..split[1].parse()? {
                                    client
                                        .spawn_entity(
                                            Uuid::new_v4().simple().to_string(),
                                            Some(split[0].to_owned()),
                                        )
                                        .await
                                        .inspect_err(|e| error!("{}", e));
                                }
                            }
                            // Write an uncompressed snapshot to disk.
                            "snap" => {
                                if args.contains(" ") {
                                    println!("Snapshot file path cannot contain spaces.");
                                    continue;
                                }

                                let snapshot = client.snapshot().await?;
                                tokio::fs::File::create_new(args)
                                    .await?
                                    .write_all(&snapshot.to_bytes(false, true)?)
                                    .await?;
                            }
                            // Initialize the cluster with default model.
                            "init" => {
                                client
                                    .initialize()
                                    .await
                                    .inspect(|_| println!("initialized successfully"))?;
                            }
                            // Show the help dialog.
                            "help" => {
                                println!("available commands:");
                                println!();
                                for &(cmd, help) in APP_COMMANDS {
                                    println!("  {:15} - {}", cmd, help);
                                }
                                println!();
                            }
                            "cfg-list" => {
                                println!(
                                    "\n\
turn_ticks              {turn_ticks}
show_on                 {show_on}
show_list               {show_list}
",
                                    turn_ticks = config.turn_steps,
                                    show_on = config.show_on,
                                    show_list = format!("{:?}", config.show_list),
                                );
                            }
                            "cfg-get" => match config.get(args) {
                                Err(e) => println!("Error: {} doesn't exist", args),
                                Ok(c) => println!("{}: {}", args, c),
                            },
                            "cfg" => {
                                let (var, val) = split_first_word(&args);
                                match config.set(var, val) {
                                    Err(e) => println!("Error: couldn't set {} to {}", var, val),
                                    Ok(()) => println!("Setting {} to {}", var, val),
                                }
                            }
                            "cfg-save" => {
                                println!("Exporting current configuration to file {}", CONFIG_FILE);
                                config.save_to_file(CONFIG_FILE).unwrap();
                            }
                            "cfg-reload" => {
                                config = match Config::new_from_file(CONFIG_FILE) {
                                    Err(e) => {
                                        if e.kind() == io::ErrorKind::NotFound {
                                            println!(
                                                "Config file {} doesn't exist, loading default config settings",
                                                CONFIG_FILE
                                            );
                                            Config::new()
                                        } else {
                                            eprintln!("There was a problem parsing the config file, loading default config settings. Details: {}", e);
                                            Config::new()
                                        }
                                    }
                                    Ok(c) => {
                                        println!(
                                            "Successfully reloaded configuration settings (found {})",
                                            CONFIG_FILE
                                        );
                                        c
                                    }
                                };
                            }

                            "show-grid" => {
                                todo!();
                            }
                            "show" => {
                                todo!();
                            }
                            "show-toggle" => {
                                if config.show_on {
                                    config.show_on = false
                                } else {
                                    config.show_on = true
                                };
                            }
                            "show-add" => {
                                config.show_add(args).unwrap();
                            }
                            "show-remove" => {
                                // TODO handle unwrap
                                config.show_remove(args).unwrap();
                            }
                            "show-clear" => {
                                config.show_list.clear();
                            }

                            "history" => {
                                let w = interface.lock_writer_erase()?;

                                for (i, entry) in w.history().enumerate() {
                                    println!("{}: {}", i, entry);
                                }
                            }

                            "quit" => break 'outer,

                            // Hidden commands.
                            "interface-set" => {
                                let d = parse_text("<input>", &line);
                                interface.evaluate_directives(d);
                            }
                            "interface-get" => {
                                if let Some(var) = interface.get_variable(args) {
                                    println!("{} = {}", args, var);
                                } else {
                                    println!("no variable named `{}`", args);
                                }
                            }
                            "interface-list" => {
                                for (name, var) in interface.lock_reader().variables() {
                                    println!("{:30} = {}", name, var);
                                }
                            }
                            "model" => {
                                let model = client.get_model().await?;
                                println!("{:#?}", model);
                            }
                            "disco" => client.shutdown().await?,

                            _ => println!("couldn't recognize input: {:?}", line),
                        }
                        if do_run_freq.is_none() && !do_run_loop {
                            interface
                                .set_prompt(create_prompt(&mut client, &config).await?.as_str())?;
                        }
                    }
                    // Handle quitting using signals and eof.
                    ReadResult::Signal(Signal::Break)
                    | ReadResult::Signal(Signal::Interrupt)
                    | ReadResult::Eof => {
                        interface.cancel_read_line();
                        do_run = false;
                        do_run_freq = None;
                        do_run_loop = false;
                        run_loop_count = 0;
                        if do_run_freq.is_none() && !do_run_loop {
                            break 'outer;
                        }
                    }
                    _ => println!("err"),
                }
            }

            // Check remote trigger.
            if let Some(oc) = &on_change {
                let mut c = oc.trigger.lock().unwrap();
                if *c == true {
                    interface.cancel_read_line();
                    match oc.action {
                        OnChangeAction::Restart => {
                            warn!("changes to project files detected, restarting...");
                            continue 'outer;
                        }
                        OnChangeAction::UpdateModel => {
                            warn!(
                                "changes to project files detected, \
                                updating current simulation instance model..."
                            );

                            // TODO: either apply the new model or restart
                            // outright.
                            todo!();

                            continue 'outer;
                        }
                    }
                }
            }
        }
    }

    println!("Leaving interactive mode.");

    if client.is_alive().await.is_ok() {
        client.shutdown().await?;
    }
    cancel.cancel();

    Ok(())
}

pub async fn create_prompt(mut driver: &mut impl AsyncClient, cfg: &Config) -> Result<String> {
    let status = driver.status().await?;
    let clock = status.clock;
    Ok(format!("[{}] ", clock,))
}

static APP_COMMANDS: &[(&str, &str)] = &[
    ("run", "Run a number of simulation ticks (hours), takes in an integer number"),
    ("runf", "Similar to `run` but doesn't listen to interupt signals, `f` stands for \"fast\" \
        (it's faster, but you will have to wait until it's finished processing)"),
    ("run-freq", "Run simulation at a constant pace, using the provided frequency"),
    ("test", "Run quick mem+proc test. Takes in a number of secs to run the average processing speed test (default=2)"),
    ("ls", "List simple variables (no lists or grids). Takes in a string argument, returns only vars that contain that string in their address"),
    ("snap", "Export current sim state to snapshot file. Takes a path to target file, relative to where endgame is running."),
    ("snapc", "Same as snap but applies compression"),
    ("cfg", "Set config variable"),
    ("cfg-get", "Print the value of one config variable"),
    ("cfg-list", "Get a list of all config variables"),
    ("cfg-save", "Save current configuration to file"),
    ("cfg-reload", "Reload current configuration from file"),
    ("show", "Print selected simulation data"),
    ("show-add", "Add to the list of simulation data to be shown"),
    (
        "show-remove",
        "Remove from the list of simulation data to be shown (by index, starting at 0)",
    ),
    (
        "show-clear",
        "Clear the list of simulation data to be shown",
    ),
    ("show-toggle", "Toggle automatic printing after each turn"),
    ("history", "Print input history"),
    ("help", "Show available commands"),
    (
        "quit",
        "Quit (NOTE: all unsaved data will be lost)",
    ),
];

fn split_first_word(s: &str) -> (&str, &str) {
    let s = s.trim();

    match s.find(|ch: char| ch.is_whitespace()) {
        Some(pos) => (&s[..pos], s[pos..].trim_start()),
        None => (s, ""),
    }
}
