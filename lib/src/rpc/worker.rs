//! Worker protocol.

use uuid::Uuid;

use crate::behavior::{BehaviorHandle, BehaviorTarget};
use crate::entity::Entity;
use crate::executor::{LocalExec, Signal};
use crate::net::CompositeAddress;
use crate::server::ServerId;
use crate::worker::WorkerId;
use crate::{
    entity, query, Address, EntityId, EntityName, EventName, Model, PrefabName, Query, Result,
    Snapshot, Var,
};

use super::{leader, server};

/// Local protocol is not meant to be serialized for over-the-wire transfer.
#[derive(Clone)]
pub enum RequestLocal {
    /// Introduce worker to leader with local channel.
    ///
    /// # Auth
    ///
    /// No auth is performed as requesting this already requires access to
    /// the runtime and relevant channels.
    ConnectToLeader(
        LocalExec<Signal<leader::RequestLocal>, Result<Signal<leader::Response>>>,
        LocalExec<Signal<RequestLocal>, Result<Signal<Response>>>,
    ),
    ConnectToServer(
        LocalExec<Signal<server::RequestLocal>, Result<Signal<server::Response>>>,
        LocalExec<Signal<RequestLocal>, Result<Signal<Response>>>,
    ),
    ConnectToWorker(),
    IntroduceLeader(LocalExec<Signal<leader::RequestLocal>, Result<Signal<leader::Response>>>),
    IntroduceServer(
        ServerId,
        LocalExec<Signal<server::RequestLocal>, Result<Signal<server::Response>>>,
    ),

    AddBehavior(BehaviorTarget, BehaviorHandle),

    Request(Request),

    Shutdown,
}

#[derive(Clone, Debug, Serialize, Deserialize, strum::Display)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Request {
    PullConfig(crate::worker::Config),
    ConnectToLeader(CompositeAddress),
    /// Introduces the calling leader to the remote worker.
    IntroduceLeader {
        /// List of exposures where leader can be reached.
        listeners: Vec<CompositeAddress>,
        /// View of the cluster at the time of introduction.
        cluster_view: Vec<(Uuid, Vec<CompositeAddress>)>,
    },
    /// Instructs the worker to reach out to a remote worker.
    ConnectToWorker(CompositeAddress),
    /// Introduces the calling worker to the remote worker.
    IntroduceWorker(WorkerId),
    GetLeader,

    /// Gets the addresses at which the worker can be reached over the network.
    GetListeners,

    /// Gets worker status.
    Status,

    NewRequirements {
        ram_mb: usize,
        disk_mb: usize,
        transfer_mb: usize,
    },

    Ping(Vec<u8>),
    MemorySize,
    Clock,
    Initialize,
    Step,
    IsBlocking {
        wait: bool,
    },

    EntityList,

    ProcessQuery(Query),

    Subscribe(Vec<query::Trigger>, Query),
    Unsubscribe(Uuid),

    /// Sets the interface receiver as "blocking" in terms of step advance
    SetBlocking(bool),

    /// Returns the current model
    GetModel,
    /// Pull the provided model replacing the currently loaded one.
    ReplaceModel(Model),
    /// Merge the provided model with the currently loaded one.
    ///
    /// This provides the ability for abstracting over things like registering
    /// new components, etc. We can just provide a new model with a single
    /// thing to get merged into the model currently loaded on the cluster.
    MergeModel(Model),
    /// This request can only come from a cluster leader. The distinction is
    /// important insomuch as for this request the worker doesn't pass the
    /// request to the leader, as it would with `ReplaceModel`.
    // TODO: provide a simpler solution to this. Perhaps the worker handler
    // should check if the message is coming from a leader or not and process
    // based on that.
    SetModel(Model),

    /// Retrieves data from a single variable
    GetVar(Address),
    /// Performs an overwrite of existing data at the given address
    SetVar(Address, Var),
    SetVars(Vec<(Address, Var)>),

    Invoke {
        events: Vec<EventName>,
        // TODO: consider whether it's needed to have more scoping options for
        // invoking events, similar to what we have for queries. This would
        // include the ability to invoke only for workers that have certain
        // entities with certain components sets available.
        global: bool,
    },

    SpawnEntity {
        name: EntityName,
        prefab: Option<PrefabName>,
    },
    DespawnEntity {
        name: EntityName,
    },
    TakeEntity(EntityName, Entity),
    GetEntity(EntityName),

    SpawnSingletonBehavior(crate::model::behavior::Behavior),

    MigrateEntities {},

    Election {
        sender_id: Uuid,
    },

    Authorize {
        token: String,
    },

    Snapshot,

    #[cfg(feature = "machine")]
    MachineLogic {
        name: String,
    },
}

impl Request {
    pub fn into_local(self) -> RequestLocal {
        RequestLocal::Request(self)
    }
}

impl Into<RequestLocal> for Request {
    fn into(self) -> RequestLocal {
        RequestLocal::Request(self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, strum::Display)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Response {
    ConnectToLeader {
        worker_id: WorkerId,
    },
    IntroduceLeader {
        worker_id: WorkerId,
    },

    IntroduceWorker(WorkerId),
    GetLeader(Option<std::net::SocketAddr>),

    GetListeners(Vec<CompositeAddress>),

    ClusterStatus {
        worker_count: usize,
    },
    NewRequirements,
    Empty,

    MemorySize(usize),
    Ping(Vec<u8>),
    GetModel(Model),
    Clock(u64),
    Step,
    IsBlocking(bool),

    Register {
        server_id: ServerId,
    },
    EntityList(Vec<EntityName>),
    Entity(Entity),
    Query(crate::QueryProduct),
    Subscribe(Uuid),
    PullProject,
    StepUntil,

    Status {
        id: Uuid,
        uptime: usize,
        worker_count: u32,
    },

    GetVar(Var),

    EntityNotFound,

    Snapshot(Snapshot),

    #[cfg(feature = "machine")]
    MachineLogic(crate::machine::Logic),
}

impl Response {
    pub fn ok(self) -> Result<()> {
        match self {
            Self::Empty => Ok(()),
            _ => Err(Error::UnexpectedResponse(format!("{:?}", self))),
        }
    }

    pub fn is_ok(self) -> bool {
        self.ok().is_ok()
    }
}

use crate::Error;

impl TryInto<Snapshot> for Response {
    type Error = Error;
    fn try_into(self) -> std::result::Result<Snapshot, Self::Error> {
        match self {
            Response::Snapshot(snapshot) => Ok(snapshot),
            _ => Err(Error::UnexpectedResponse(self.to_string())),
        }
    }
}

impl TryInto<Model> for Response {
    type Error = Error;

    fn try_into(self) -> std::result::Result<Model, Self::Error> {
        match self {
            Response::GetModel(model) => Ok(model),
            _ => Err(Error::UnexpectedResponse(self.to_string())),
        }
    }
}

impl TryInto<Var> for Response {
    type Error = Error;

    fn try_into(self) -> std::result::Result<Var, Self::Error> {
        match self {
            Response::GetVar(var) => Ok(var),
            _ => Err(Error::UnexpectedResponse(self.to_string())),
        }
    }
}

impl TryInto<crate::QueryProduct> for Response {
    type Error = Error;

    fn try_into(self) -> std::result::Result<crate::QueryProduct, Self::Error> {
        match self {
            Response::Query(product) => Ok(product),
            _ => Err(Error::UnexpectedResponse(self.to_string())),
        }
    }
}
