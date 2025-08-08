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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct NodeConfig {
    pub listeners: Vec<CompositeAddress>,

    pub single_threaded: bool,
    pub max_memory: usize,
    pub workers_per_thread: u16,

    pub acl: FnvHashMap<String, Permissions>,
}

impl Default for NodeConfig {
    fn default() -> Self {
        let mut acl = FnvHashMap::default();
        acl.insert("root".to_string(), Permissions::root());
        Self {
            listeners: vec![],
            single_threaded: false,
            max_memory: 1000,
            workers_per_thread: 2,
            acl,
        }
    }
}

#[derive(Clone)]
pub struct NodeHandle {
    pub config: NodeConfig,
    pub ctl: LocalExec<rpc::node::Request, Result<rpc::node::Response>>,
}

#[async_trait::async_trait]
impl Executor<rpc::node::Request, rpc::node::Response> for NodeHandle {
    async fn execute(&self, req: rpc::node::Request) -> Result<rpc::node::Response> {
        self.ctl.execute(req).await?.map_err(|e| e.into())
    }
}

#[derive(Clone)]
pub struct RemoteNode {
    pub exec: RemoteExec<rpc::node::Request, Result<rpc::node::Response>>,
}

#[derive(Clone, Default)]
pub struct Node {
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
pub fn spawn(config: NodeConfig, mut cancel: CancellationToken) -> Result<NodeHandle> {
    info!("spawning server task, listeners: {:?}", config.listeners);

    let (local_exec, mut local_stream, _) = LocalExec::new(20);
    let (net_exec, mut net_stream, _) = LocalExec::new(20);

    net::spawn_listeners(&config.listeners, net_exec.clone(), cancel.clone())?;

    // Spawn the main handler task.
    tokio::spawn(async move {
        let mut node = Node::default();

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

    Ok(NodeHandle {
        config,
        ctl: local_exec,
    })
}

fn handle_request(
    req: rpc::node::Request,
    node: &mut Node,
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
                let server_handle = server::spawn(config, worker_handle.clone(), cancel.clone())?;
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
