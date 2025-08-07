//! Defines protocols for communication with different network constructs.

use std::{str::FromStr, time::Duration};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    leader::LeaderId,
    server::{ClientId, ServerId},
    worker::WorkerId,
    Error,
};

pub mod leader;
pub mod node;
pub mod server;
pub mod worker;

pub mod behavior;
#[cfg(feature = "machine")]
pub mod machine;

pub mod msg;

/// Listing of all possible cluster parties along with their ids.
// TODO: not really rpc-specific, maybe move elsewhere.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Participant {
    Leader,
    Worker(WorkerId),
    Server(ServerId),
    Client(ClientId),
}

impl TryFrom<Caller> for Participant {
    type Error = Error;

    fn try_from(caller: Caller) -> Result<Self, Self::Error> {
        match caller {
            Caller::Participant(participant) => Ok(participant),
            _ => Err(Error::FailedConversion(format!(
                "unable to convert caller into participant: {:?}",
                caller
            ))),
        }
    }
}

/// Listing of all possible callers. Includes cluster participants as well as
/// any additional constructs able to issue calls.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Caller {
    Participant(Participant),
    SimHandle,
    Behavior,
    #[default]
    Unknown,
}

impl From<Participant> for Caller {
    fn from(p: Participant) -> Self {
        Self::Participant(p)
    }
}

impl Caller {
    pub fn id(&self) -> Uuid {
        match self {
            Self::Participant(p) => match p {
                Participant::Worker(id) => *id,
                Participant::Server(id) => *id,
                Participant::Client(id) => *id,
                // NOTE: Leader participant is always unique, so it doesn't
                // have an identifier ascribed.
                Participant::Leader => Uuid::from_u128(1),
            },
            // NOTE: non-cluster-participants cannot be uniquely identified.
            Self::SimHandle => Uuid::nil(),
            Self::Behavior => Uuid::max(),
            Self::Unknown => Uuid::from_u128(42),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkHop {
    /// Identifier of the participant that obeserved and registered the
    /// network hop.
    pub observer: Caller,
    /// Time between original issuing of the call and registering of this hop.
    pub delta_time: Duration,
}

/// Describes additional information about the call as it passes through the
/// cluster.
///
/// Supplies information about the original caller, the relay chain of the
/// request as it went through the network, timing information, etc.
///
/// # Context lifetime
///
/// The context is designed to be created at the original callsite and to
/// travel alongside the payload, eventually making it back to the caller.
///
/// This way the caller as well as different participants processing an
/// in-flight call can make informed judgements about routing, state of the
/// network, etc.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Context {
    /// Identifier of the original calling party.
    pub origin: Caller,
    /// Explicit target of the call. This is not always defined, e.g. when
    /// broadcasting to all participants or when the call is not expected to
    /// travel through the cluster.
    pub target: Option<Participant>,

    /// Date and time when the call was originally made.
    pub initiated_at: DateTime<Utc>,
    /// Network hops made so far.
    pub hops: Vec<NetworkHop>,
}

impl Context {
    pub fn new(origin: Caller) -> Self {
        Self {
            origin,
            target: None,
            initiated_at: Utc::now(),
            hops: vec![],
        }
    }

    /// Checks whether the origin or registered observers of the signal include
    /// a particular worker.
    pub fn went_through_worker(&self, id: &Uuid) -> bool {
        &self.origin.id() == id || self.worker_observers().contains(&id)
    }

    /// Checks whether the registered observers of the signal include a leader
    /// participant.
    //
    // This can be important when the request was initiated by a worker to be
    // "proxied" through the leader to one or more workers. We need this
    // information to prevent other "non-peering" workers from also trying to
    // proxy the request through the leader, resulting in an infnite loop.
    pub fn went_through_leader(&self) -> bool {
        if let Caller::Participant(Participant::Leader) = self.origin {
            true
        } else if self.hops.iter().any(|hop| {
            if let Caller::Participant(Participant::Leader) = hop.observer {
                true
            } else {
                false
            }
        }) {
            true
        } else {
            false
        }
    }

    /// Returns a list of all registered worker observers.
    pub fn worker_observers(&self) -> Vec<WorkerId> {
        self.hops
            .iter()
            .filter_map(|hop| {
                if let Caller::Participant(Participant::Worker(id)) = hop.observer {
                    Some(id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    }

    /// Counts number of hops where observers were workers.
    pub fn worker_hop_count(&self) -> usize {
        self.hops
            .iter()
            .filter(|hop| {
                if let Caller::Participant(Participant::Worker(_)) = hop.observer {
                    true
                } else {
                    false
                }
            })
            .count()
    }

    /// Checks whether recorded observers include the specified target.
    ///
    /// Returns `None` if the target is not specified.
    pub fn reached_target(&self) -> bool {
        if let Some(target) = &self.target {
            self.hops.iter().any(|hop| {
                if let Caller::Participant(observer) = &hop.observer {
                    observer == target
                } else {
                    false
                }
            })
        } else {
            false
        }
    }
}
