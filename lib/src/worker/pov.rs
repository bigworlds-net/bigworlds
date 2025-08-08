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
    /// Default situation for a newly spawned worker that wasn't part of any
    /// cluster yet.
    ///
    /// TODO: perhaps it should be possible for a worker to never be directly
    /// connected to a leader. It could relay the leader messages via a remote
    /// worker that it would be connected to.
    #[default]
    Never,
    /// Active connection to the cluster leader.
    Connected(Leader),
    /// Previous leader was lost leading to workers holding election.
    Election,
    /// Lost leader while not being connected to any other cluster
    /// participants. Currently waiting for incoming worker calls.
    ///
    /// NOTE: This situation will persist for some amount of time. During that
    /// time it's possible that the worker will get contacted by remaining
    /// cluster participants, or that it will be successful in reconnecting to
    /// the leader at the same address it was exposed at before (e.g. leader
    /// restarted). Alternativaly after the wait is over the worker will either
    /// be shut down or spawn a new leader just for itself.
    OrphanedWaiting,
}

#[derive(Clone, Debug)]
pub struct Leader {
    pub exec: LeaderExec,
    pub worker_id: WorkerId,
    pub listeners: Vec<CompositeAddress>,
}

#[async_trait::async_trait]
impl Executor<Signal<rpc::leader::Request>, Signal<rpc::leader::Response>> for Leader {
    async fn execute(
        &self,
        sig: Signal<rpc::leader::Request>,
    ) -> Result<Signal<rpc::leader::Response>> {
        match &self.exec {
            LeaderExec::Remote(remote_exec) => remote_exec.execute(sig).await?,
            LeaderExec::Local(local_exec) => {
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

#[derive(Clone)]
pub struct Server {
    pub worker_id: WorkerId,
    pub server_id: ServerId,
    pub exec: ServerExec,
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

// TODO: track additional information about remote workers
#[derive(Clone, Debug)]
pub struct OtherWorker {
    /// Local worker id.
    pub local_id: WorkerId,

    /// Globally unique uuid self-assigned by the other worker.
    pub id: WorkerId,
    /// Executor for directly passing request to the other worker.
    pub exec: WorkerExec,
}

#[async_trait::async_trait]
impl Executor<Request, Response> for OtherWorker {
    async fn execute(&self, req: Request) -> Result<Response> {
        match &self.exec {
            WorkerExec::Remote(remote_exec) => remote_exec
                .execute(
                    Signal::from(req).originating_at(Participant::Worker(self.local_id).into()),
                )
                .await?
                // Discard the context.
                .map(|r| r.into_payload()),
            WorkerExec::Local(local_exec) => local_exec
                .execute(Signal::from(RequestLocal::Request(req)))
                .await
                .map_err(|e| Error::Other(e.to_string()))?
                .map_err(|e| Error::Other(e.to_string()))
                .map(|s| s.into_payload()),
        }
    }
}
