use crate::{
    leader::LeaderExec,
    net::CompositeAddress,
    rpc::{
        self,
        worker::{Request, RequestLocal, Response},
        Participant,
    },
    server::{ServerExec, ServerId},
    Error, Executor, Result, Signal,
};

use super::{WorkerExec, WorkerId};

#[derive(Clone, Debug, Default)]
pub enum LeaderSituation {
    /// Unaware of the leader, meaning there's no direct connection and there
    /// was no attempt to contact the leader indirectly.
    ///
    /// This is the default situation for a newly spawned worker.
    #[default]
    Unaware,
    /// Attempted contact but failed.
    Unreachable,
    /// Successfully contacted the leader at some point in the past, through
    /// indirect means.
    ///
    /// This variant holds a timestamp (seconds) of the last contact.
    // TODO: consider storing connected participant through which we got the
    // last contact. Also consider storing the latency for last contact.
    Aware(u32),
    /// Holds a direct connection to the cluster leader.
    Connected(LeaderExec),
    /// Previous leader was lost leading to workers holding election.
    ///
    /// This situation emerges when enough workers mark leader as unreachable.
    /// Based on policy
    Election,
    /// Lost direct leader connection while not being connected to any other
    /// cluster participants. Currently waiting for incoming worker calls.
    ///
    /// This situation also emerges when the worker loses all last connection
    /// to another worker through which it could contact leader indirectly.
    ///
    /// NOTE: This situation will persist for some amount of time. During that
    /// time it's possible that the worker will get contacted by remaining
    /// cluster participants, or that it will be successful in reconnecting to
    /// the leader at the same address it was exposed at before (e.g. leader
    /// restarted). Alternatively after the wait is over the worker will either
    /// be shut down or spawn a new leader just for itself.
    OrphanedWaiting,
}

#[derive(Clone, Debug, Default)]
pub struct Leader {
    pub situation: LeaderSituation,
    pub listeners: Vec<CompositeAddress>,
}

impl From<LeaderSituation> for Leader {
    fn from(situation: LeaderSituation) -> Self {
        Self {
            situation,
            ..Default::default()
        }
    }
}

#[async_trait::async_trait]
impl Executor<Signal<rpc::leader::Request>, Signal<rpc::leader::Response>> for Leader {
    async fn execute(
        &self,
        sig: Signal<rpc::leader::Request>,
    ) -> Result<Signal<rpc::leader::Response>> {
        if let LeaderSituation::Connected(exec) = &self.situation {
            exec.execute(sig).await
        } else {
            Err(Error::LeaderNotConnected(format!(
                "no direct connection available, ctx: {:?}",
                sig.ctx,
            )))
        }
    }
}

#[derive(Clone, Debug)]
pub enum WorkerSituation {
    /// Made indirect contact with the worker at least once in the past.
    ///
    /// Holds a timestamp (seconds) of the last contact.
    // TODO: perhaps hold last path (list of participant ids) the worker
    // contacted through.
    Aware(u32),
    /// Holds a direct connection to the worker.
    Connected(WorkerExec),
    /// Attempted contact but failed.
    Unreachable,
}

impl Default for WorkerSituation {
    fn default() -> Self {
        Self::Aware(chrono::Utc::now().timestamp() as u32)
    }
}

#[derive(Clone, Debug)]
pub struct OtherWorker {
    /// Globally unique uuid self-assigned by the other worker.
    pub id: WorkerId,
    /// Current status of the other worker as seen by the local worker.
    pub situation: WorkerSituation,
    pub listeners: Vec<CompositeAddress>,
}

#[async_trait::async_trait]
impl Executor<Signal<Request>, Signal<Response>> for OtherWorker {
    async fn execute(&self, sig: Signal<Request>) -> Result<Signal<Response>> {
        if let WorkerSituation::Connected(exec) = &self.situation {
            match exec {
                WorkerExec::Remote(remote_exec) => remote_exec.execute(sig).await?,
                WorkerExec::Local(local_exec) => local_exec
                    .execute(Signal::new(sig.payload.into_local(), sig.ctx))
                    .await
                    .map_err(|e| Error::Other(e.to_string()))?
                    .map_err(|e| Error::Other(e.to_string())),
            }
        } else {
            Err(Error::WorkerNotConnected(format!(
                "no direct connection available, ctx: {:?}",
                sig.ctx,
            )))
        }
    }
}

#[derive(Clone)]
pub struct Server {
    pub id: ServerId,
    /// From the worker point of view, server is always defined by its
    /// connection to the worker.
    pub exec: ServerExec,
    pub listeners: Vec<CompositeAddress>,
}

#[async_trait::async_trait]
impl Executor<Signal<rpc::server::Request>, Signal<rpc::server::Response>> for Server {
    async fn execute(
        &self,
        sig: Signal<rpc::server::Request>,
    ) -> Result<Signal<rpc::server::Response>> {
        match &self.exec {
            ServerExec::Remote(remote_exec) => remote_exec.execute(sig.into()).await?,
            ServerExec::Local(local_exec) => {
                let sig = Signal::new(sig.payload.into(), sig.ctx);
                local_exec
                    .execute(sig)
                    .await
                    .map_err(|e| Error::Other(e.to_string()))?
                    .map_err(|e| Error::Other(e.to_string()))
            }
        }
    }
}
