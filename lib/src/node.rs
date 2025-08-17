//! `node` is a runtime wrapper that can instantiate workers, leaders,
//! services within a context of a single process, based on incoming
//! requests.
//!
//! The main idea behind the `node` interface is to be able to incorporate
//! new compute as easily as possible. Running `bigworlds node` and providing
//! the newly gotten listener address to a controller (e.g. the platform
//! dashboard) should be enough.
//!
//! A single node can optionally house cluster participants from multiple
//! different clusters, effectively handling multiple worlds concurrently.
//!
//!
//! # Callbacks with spawn requests
//!
//! Some constructs may want to call back the node and request spawning other
//! constructs on the node.
//!
//! Such is the case with workers. When cluster leader is lost, workers need
//! to quickly spawn a new one. They will try to find a suitable node among
//! themselves, requesting node configuration to assess node fitness and to
//! make sure it supports spawning leaders in the first place.

mod config;
mod handle;
mod pov;

pub use config::Config;
pub use handle::Handle;
pub use pov::RemoteNode;

use std::net::SocketAddr;

use fnv::FnvHashMap;
use tokio::sync::oneshot;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::executor::{Executor, LocalExec};
use crate::net::{CompositeAddress, ConnectionOrAddress, Encoding, Transport};
use crate::rpc::node::{Request, Response};
use crate::time::Duration;
use crate::util::{decode, encode};
use crate::{leader, net, rpc, server, worker, Error, RemoteExec, Result};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Permissions {
    pub create_workers: bool,
    pub create_leaders: bool,
}

impl Permissions {
    pub fn root() -> Self {
        Self {
            create_workers: true,
            create_leaders: false,
        }
    }
}

#[derive(Clone, Default)]
pub struct State {
    pub workers: Vec<worker::Handle>,
    pub leaders: Vec<leader::Handle>,
    pub servers: Vec<server::Handle>,

    pub nodes: Vec<RemoteNode>,
}

/// Spawns a `Node` task on the current runtime.
///
/// # Node handle
///
/// The returned handle can be used to send requests to the `Node`.
///
/// Request format is the same as with remote requests coming through the
/// network listener.
pub fn spawn(config: Config, mut cancel: CancellationToken) -> Result<Handle> {
    info!("spawning server task, listeners: {:?}", config.listeners);

    let (local_exec, mut local_stream, _) = LocalExec::new(20);
    let (net_exec, mut net_stream, _) = LocalExec::new(20);

    net::spawn_listeners(&config.listeners, net_exec.clone(), cancel.clone())?;

    // Spawn the main handler task.
    tokio::spawn(async move {
        let mut node = State::default();

        let mut cancel_c = cancel.clone();
        loop {
            tokio::select! {
                Some(((addr, req), s)) = net_stream.next() => {
                    let req = match decode(&req, Encoding::Bincode) {
                        Ok(r) => r,
                        Err(e) => {
                            error!("failed decoding node request (bincode)");
                            continue;
                        }
                    };
                    debug!("node: received net request: {:?}", req);

                    // Identify caller and check privileges.

                    let resp = handle_request(req, &mut node, cancel.clone());
                    s.send(encode(resp.map_err(|e| e.to_string()), Encoding::Bincode).unwrap());
                },
                Some((req, s)) = local_stream.next() => {
                    debug!("node: received local request: {:?}", req);
                    let resp = handle_request(req, &mut node, cancel.clone());
                    s.send(resp);
                },
                _ = cancel_c.cancelled() => break,
            };
        }
    });

    Ok(Handle {
        config,
        ctl: local_exec,
    })
}

fn handle_request(
    req: rpc::node::Request,
    node: &mut State,
    cancel: CancellationToken,
) -> Result<Response> {
    match req {
        Request::Status => Ok(Response::Status {
            worker_count: node.workers.len(),
        }),
        Request::SpawnWorker(worker_config, server_config) => {
            info!("node: spawning worker: {:?}", worker_config);
            let mut worker_handle = worker::spawn(worker_config.clone(), cancel.clone())?;
            let server_exec = worker_handle.server_exec.clone();
            node.workers.push(worker_handle.clone());

            let mut server_listeners = vec![];
            if let Some(config) = server_config {
                info!("node: spawning worker-backed server: {:?}", config);
                server_listeners.extend(config.listeners.clone());
                let server_handle = server::spawn(config, cancel.clone())?;
                node.servers.push(server_handle);
            }

            Ok(rpc::node::Response::SpawnWorker {
                worker_listeners: worker_config.listeners,
                server_listeners,
            })
        }
        Request::SpawnLeader(config) => {
            info!("node: spawning leader: {:?}", config);
            let leader_handle = leader::spawn(config.clone(), cancel.clone())?;

            // TODO: return a list of actually realized listeners
            Ok(Response::SpawnLeader {
                listeners: config.listeners,
            })
        }
    }
}
