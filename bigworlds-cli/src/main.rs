//! Command line program for working with `bigworlds` simulations.

#![allow(unused)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate serde;

mod cli;
mod interactive;
mod node;
mod run;
mod tracing;
mod util;

mod client;
mod leader;
mod server;
mod worker;

use std::time::Duration;

use colored::*;

fn main() {
    let mut runtime = tokio::runtime::Builder::new_multi_thread();

    // Allow specifying number of worker threads for the runtime.
    //
    // NOTE: one can also use the `TOKIO_WORKER_THREADS` environment variable
    // to set this value instead of passing it as a CLI argument..
    let matches = cli::arg_matches();
    if let Some(threads) = matches.get_one::<String>("threads") {
        runtime.worker_threads(threads.parse().expect("badly formed threads argument"));
    }

    let runtime = runtime
        .enable_all()
        .build()
        .expect("failed building tokio runtime");

    runtime.block_on(async move {
        // Run the program based on user input
        match cli::start(matches).await {
            Ok(_) => (),
            Err(e) => {
                print!("{}{}\n", "error: ".red(), e);
                if e.root_cause().to_string() != e.to_string() {
                    println!("Caused by:\n{}", e.root_cause())
                }
            }
        }
    });

    debug!(
        "runtime: shutting down ({} tasks remain active)",
        runtime.metrics().num_alive_tasks()
    );
    runtime.shutdown_timeout(Duration::from_secs(1));
}
