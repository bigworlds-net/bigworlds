use std::{
    fs::File,
    hash::Hash,
    io::{Read, Write},
};

use libloading::Library;

use crate::{
    behavior, model::behavior::Artifact, rpc, rpc::Signal, Error, EventName, LocalExec, Result,
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
    debug!(
        "opening at lib_path: {}, truncating (rewriting) existing artifact if it exists",
        lib_path
    );

    // TODO! rewriting existing artifacts creates issues when having multiple
    // workers.

    // TODO: figure out a clever way to perhaps not replace it
    // everytime, or at least provide some config variable for
    // controlling this behavior.
    if !std::fs::exists(&lib_path)? {
        File::create(&lib_path)
            .unwrap()
            .write_all(&artifact.bytes)
            .unwrap();
    } else {
        // Check the artifact hash and decide whether overwrite.

        let mut file = File::open(&lib_path)?;
        let mut buf = vec![];
        file.read_to_end(&mut buf)?;
        let artifact_: Artifact = buf.into();

        let mut s = std::hash::DefaultHasher::new();
        artifact_.hash(&mut s);
        let file_hash = std::hash::Hasher::finish(&s);

        let mut s = std::hash::DefaultHasher::new();
        artifact.hash(&mut s);
        let hash = std::hash::Hasher::finish(&s);

        if file_hash != hash {
            File::create(&lib_path)
                .unwrap()
                .write_all(&artifact.bytes)
                .unwrap();
        }
    }

    // Load the library.
    let lib = unsafe { libloading::Library::new(&lib_path).unwrap() };

    // Run the function as a separate `behavior`.
    let handle = unsafe {
        if synced {
            let function: libloading::Symbol<crate::behavior::BehaviorFnSynced> =
                lib.get(entry.as_bytes()).unwrap();
            debug!("spawning synced dynlib behavior: {}", lib_path);
            let handle =
                behavior::spawn_synced(*function, name.to_owned(), triggers, worker_exec.clone())?;
            Some(handle)
        } else {
            let function: libloading::Symbol<crate::behavior::BehaviorFnUnsynced> =
                lib.get(entry.as_bytes()).unwrap();
            debug!("spawning unsynced dynlib behavior: {}", lib_path);
            behavior::spawn_unsynced(*function, proc_tx.subscribe(), worker_exec.clone())?;
            None
        }
    };

    Ok((lib, handle))
}
