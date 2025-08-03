use fnv::FnvHashMap;
use serde::{Deserialize, Serialize};

use crate::executor::{LocalExec, Signal};
use crate::leader::Status;
use crate::net::CompositeAddress;
use crate::worker::WorkerId;
use crate::{EntityName, Error, Model, PrefabName, Result};

use super::worker;

pub type Token = uuid::Uuid;

#[derive(Clone)]
pub enum RequestLocal {
    /// Introduce worker to leader with local channel.
    ///
    /// # Auth
    ///
    /// No auth is performed as requesting this already requires access to
    /// the runtime and relevant channels.
    ConnectToWorker(
        LocalExec<Signal<worker::RequestLocal>, Result<Signal<worker::Response>>>,
        LocalExec<Signal<RequestLocal>, Result<Signal<Response>>>,
    ),
    ConnectAndRegisterWorker(
        LocalExec<Signal<worker::RequestLocal>, Result<Signal<worker::Response>>>,
    ),
    Request(Request),

    Shutdown,
}

impl From<Request> for RequestLocal {
    fn from(value: Request) -> Self {
        Self::Request(value)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Request {
    /// Instruct the leader to reach out to the worker at provided address.
    ConnectToWorker(CompositeAddress),
    /// Introduce the calling worker to the leader.
    // TODO: auth.
    IntroduceWorker(WorkerId),
    DisconnectWorker,

    /// Request full status update of the leader.
    Status,

    /// Pull incoming model replacing it as current model.
    ReplaceModel(Model),
    /// Merge incoming model into the currently loaded model.
    MergeModel(Model),

    /// Make leader initialize the cluster, optionally using the provided
    /// scenario.
    Initialize {
        scenario: Option<String>,
    },
    /// Step through the simulation.
    Step,

    /// Request the current simulation clock value on the leader.
    Clock,
    /// Request a list of all currently connected workers.
    GetWorkers,
    /// Request the complete model from leader.
    Model,

    ReadyUntil(usize),

    /// Spawn entity based on the provided name prefab.
    SpawnEntity {
        name: EntityName,
        prefab: Option<PrefabName>,
    },
    /// Remove entity based on the provided name.
    RemoveEntity {
        name: EntityName,
    },

    /// Request simple echoing of sent bytes.
    Ping(Vec<u8>),

    /// Broadcast a worker request to all connected workers.
    ///
    /// This can be used by workers that are only connected to the leader
    /// and not directly to other workers.
    // Broadcast(super::worker::Request),

    /// Pass a worker request to one of the connected workers selected
    /// at random.
    ///
    /// This can be used by workers that are only connected to the leader
    /// and not directly to other workers.
    WorkerProxy(super::worker::Request),

    /// Request the leader to calculate migration tresholds and potentially
    /// migrate entities.
    ReorganizeEntities {
        shuffle: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, strum::Display)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Response {
    Empty,

    Status(Status),

    WorkerStatus { uptime: usize },
    ClusterStatus { worker_count: usize },

    Ping(Vec<u8>),
    Connect,
    Register { worker_id: WorkerId },
    Clock(usize),
    GetWorkers(FnvHashMap<WorkerId, Vec<CompositeAddress>>),
    Model(Model),

    ReplaceModel,

    StepUntil,

    // Broadcast(super::worker::Response),
    WorkerProxy(super::worker::Response),
}

impl TryInto<Model> for Response {
    type Error = Error;

    fn try_into(self) -> std::result::Result<Model, Self::Error> {
        match self {
            Response::Model(model) => Ok(model),
            _ => Err(Error::UnexpectedResponse(self.to_string())),
        }
    }
}
