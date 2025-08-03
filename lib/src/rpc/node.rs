use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::executor::LocalExec;
use crate::net::CompositeAddress;
use crate::{leader, rpc, worker, Result};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Status,
    SpawnWorker(worker::Config),
    SpawnLeader(leader::Config),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Response {
    Status {
        worker_count: usize,
    },
    SpawnWorker {
        listeners: Vec<CompositeAddress>,
        // TODO: also optionally provide server address in the response.
    },
    SpawnLeader {
        listeners: Vec<CompositeAddress>,
    },
}
