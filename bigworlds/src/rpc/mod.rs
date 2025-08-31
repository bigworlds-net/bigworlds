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

/// Wraps request along with call context for tracking destinations and timing.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Signal<T> {
    pub payload: T,
    pub ctx: Context,
}

/// Basic conversion implementation for creating context-less cluster signals.
impl<T> From<T> for Signal<T> {
    fn from(payload: T) -> Self {
        Self {
            payload,
            ctx: Context::new(Caller::Unknown),
        }
    }
}

impl<T> Signal<T> {
    pub fn new(payload: T, ctx: Context) -> Self {
        Self { payload, ctx }
    }

    /// Convenience function for providing default context while also explicitly
    /// specifying signal origin.
    pub fn originating_at(mut self, caller: Caller) -> Self {
        self.ctx.origin = caller;
        self
    }

    pub fn with_target(mut self, target: Participant) -> Self {
        self.ctx.target = Some(target);
        self
    }

    /// Returns the embedded payload discarding context.
    pub fn into_payload(self) -> T {
        self.payload
    }

    /// Same as `into_payload` but carries the significance of discarding the context.
    pub fn discard_context(self) -> T {
        self.payload
    }

    /// Returns the context discarding the payload.
    pub fn into_context(self) -> Context {
        self.ctx
    }
}

/// Listing of all possible cluster parties along with their ids.
// TODO: not really rpc-specific, maybe move elsewhere.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Participant {
    Leader,
    Worker(WorkerId),
    Server(ServerId),
    // TODO: consider removing client, it doesn't really participate in the
    // inner cluster.
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
#[derive(Clone, Debug, strum::Display, Default, PartialEq, Serialize, Deserialize)]
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

/// Abstracts over a single instance of the call being observed by cluster
/// participant.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkHop {
    /// Identifier of the participant that obeserved and registered the
    /// network hop.
    pub observer: Participant,
    /// Time between original issuing of the call and registering of this hop,
    /// expressed in milliseconds.
    ///
    /// We store it this way because we want to both have micro-second level
    /// measurement and save space by not doing u64.
    ///
    /// NOTE: with `u32::MAX / 1000000` at around 65, max supported delta here
    /// is slightly over a minute. This is currently the hard limit for call
    /// timings.
    ///
    // NOTE: storing delta time between hops was considered but it appeared to
    // become unwieldy with more complex non-seqeuential broadcast patterns.
    pub delta_time_micros: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub enum Delivery {
    #[default]
    Unlimited,
    Limited(u8),
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
// TODO: consider changing the name to `CallInfo` or similar.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Context {
    /// Unique identifier of the call.
    pub id: Uuid,

    /// Delivery strategy for the call.
    ///
    /// Different types of cluster operations can require varying kinds of
    /// delivery limits. For example with epidemic broadcast we need to make
    /// sure each call is only processed once.
    pub delivery: Delivery,

    /// Identifier of the original calling party.
    pub origin: Caller,
    /// Explicit target of the call. This is not always defined, e.g. when
    /// broadcasting to all participants or when the call is not expected to
    /// travel through the cluster.
    pub target: Option<Participant>,

    /// Time at which the call was originally made, expressed as unix timestamp
    /// (micros).
    pub initiated_at: u64,
    /// Network hops made so far.
    pub hops: Vec<NetworkHop>,
}

impl Context {
    pub fn new(origin: Caller) -> Self {
        Self {
            id: Uuid::new_v4(),
            delivery: Delivery::default(),
            origin,
            target: None,
            initiated_at: Utc::now().timestamp_micros() as u64,
            hops: vec![],
        }
    }

    pub fn delivery(mut self, limit: Option<u8>) -> Self {
        if let Some(limit) = limit {
            self.delivery = Delivery::Limited(limit);
        } else {
            self.delivery = Delivery::Unlimited;
        }
        self
    }

    /// Returns the last known participant observer.
    pub fn last_observer(&self) -> Option<&Participant> {
        self.hops.last().map(|hop| &hop.observer)
    }

    /// Registers network hop with a particular participant observer.
    pub fn register_hop(mut self, observer: Participant) -> Self {
        // Calculate time since original initiation, then substract total time
        // so far.
        let delta_time_micros = (Utc::now().timestamp_micros() as u64 - self.initiated_at) as u32
            - self.total_transfer_time_micros();
        let hop = NetworkHop {
            observer,
            delta_time_micros,
        };
        self.hops.push(hop);
        self
    }

    /// Merges incoming list of hops into the current context.
    pub fn merge_hops(&mut self, other: &Vec<NetworkHop>) {
        self.hops
            .extend(other.iter().skip(self.hops.len()).cloned());
    }

    pub fn merge_hops_unique(&mut self, other: &Vec<NetworkHop>) {
        for other_hop in other {
            if !self
                .hops
                .iter()
                .any(|hop| hop.observer == other_hop.observer)
            {
                self.hops.push(other_hop.clone());
            }
        }
    }

    /// Merges incoming list of hops into the current context based on timings.
    pub fn merge_hops_timebased(&mut self, other: &Vec<NetworkHop>) {
        // Find the largest delta time.
        let mut largest = 0;
        for hop in &self.hops {
            if hop.delta_time_micros > largest {
                largest = hop.delta_time_micros;
            }
        }

        // Find other hops larger than largest. This way we're only merging in
        // hops that happened after the last hop on the self.
        for other_hop in other {
            if other_hop.delta_time_micros > largest {
                // Add the other hop on the self.
                self.hops.push(other_hop.to_owned());
            }
        }
    }

    pub fn clear_hops(mut self) -> Self {
        self.hops.clear();
        self
    }

    /// Returns total transfer time by reading the delta of the last hop.
    pub fn total_transfer_time_micros(&self) -> u32 {
        if let Some(last) = self.hops.last() {
            last.delta_time_micros
        } else {
            0
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
            if let Participant::Leader = hop.observer {
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
                if let Participant::Worker(id) = hop.observer {
                    Some(id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    }

    /// Counts number of hops where observers were workers.
    ///
    /// NOTE: the original caller is not included in the total hop count.
    pub fn worker_hop_count(&self) -> usize {
        self.hops
            .iter()
            .filter(|hop| {
                if let Participant::Worker(_) = hop.observer {
                    true
                } else {
                    false
                }
            })
            .count()
    }

    pub fn leader_hop_count(&self) -> usize {
        self.hops
            .iter()
            .filter(|hop| {
                if let Participant::Leader = hop.observer {
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
            self.hops.iter().any(|hop| &hop.observer == target)
        } else {
            false
        }
    }
}
