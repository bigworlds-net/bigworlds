use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::executor::LocalExec;
use crate::net::CompositeAddress;
use crate::{leader, rpc, server, worker, Result};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Status,
    /// Spawn a new worker. Optionally also spawn a worker-backed server.
    SpawnWorker(worker::Config, Option<server::Config>),
    SpawnLeader(leader::Config),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Response {
    Status {
        worker_count: usize,
    },
    SpawnWorker {
        worker_listeners: Vec<CompositeAddress>,
        server_listeners: Vec<CompositeAddress>,
    },
    SpawnLeader {
        listeners: Vec<CompositeAddress>,
    },
}
