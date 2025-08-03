use std::{fs::File, io::Write};

use libloading::Library;

use crate::{
    behavior, executor::Signal, model::behavior::Artifact, rpc, Error, EventName, LocalExec, Result,
};

use super::BehaviorHandle;

pub fn spawn(
    name: &String,
    artifact: &Artifact,
    entry: &String,
    synced: bool,
    debug: bool,
    triggers: Vec<EventName>,
    worker_exec: LocalExec<Signal<rpc::worker::Request>, Result<Signal<rpc::worker::Response>>>,
    proc_tx: tokio::sync::broadcast::Sender<rpc::behavior::Request>,
) -> Result<(Library, Option<BehaviorHandle>)> {
    // Save the library to disk at a predetermined location.
    let lib_path = format!(
        "{}/.bigworlds/{}",
        // TODO: apparently this is deprecated
        std::env::home_dir()
            .ok_or(Error::Other("unable to find home dir".to_owned()))?
            .to_string_lossy(),
        name
    );
    println!(
        "opening at lib_path: {}, truncating (rewriting) existing artifact if it exists",
        lib_path
    );
    File::create(&lib_path)
        .unwrap()
        .write_all(&artifact.bytes)
        .unwrap();
    // TODO: figure out a clever way to perhaps not replace it
    // everytime, or at least provide some config variable for
    // controlling this behavior.
    // if !std::fs::exists(&lib_path)? {
    // } else {
    // }

    // Load the library
    let lib = unsafe { libloading::Library::new(&lib_path).unwrap() };

    // Run the function as a separate `behavior`
    let handle = unsafe {
        if synced {
            let function: libloading::Symbol<crate::behavior::BehaviorFnSynced> =
                lib.get(entry.as_bytes()).unwrap();
            info!("spawning synced dynlib behavior: {}", lib_path);
            let handle =
                behavior::spawn_synced(*function, name.to_owned(), triggers, worker_exec.clone())?;
            Some(handle)
        } else {
            let function: libloading::Symbol<crate::behavior::BehaviorFnUnsynced> =
                lib.get(entry.as_bytes()).unwrap();
            info!("spawning unsynced dynlib behavior: {}", lib_path);
            behavior::spawn_unsynced(*function, proc_tx.subscribe(), worker_exec.clone())?;
            None
        }
    };

    Ok((lib, handle))
}
