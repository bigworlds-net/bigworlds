use crate::{
    net::CompositeAddress,
    rpc::{self, Participant},
    worker::{WorkerExec, WorkerId},
    EntityName, Error, Executor, Result, Signal,
};

/// Single worker as seen by the leader.
#[derive(Clone, Debug)]
pub struct Worker {
    pub id: WorkerId,

    /// Tracked list of entities that currently exist on the worker.
    pub entities: Vec<EntityName>,

    /// Executor abstracting over the transmission medium.
    pub exec: WorkerExec,

    /// List of known listeners the worker can be reached through.
    pub listeners: Vec<CompositeAddress>,
}

impl Worker {
    pub fn new(id: WorkerId, exec: WorkerExec) -> Self {
        Self {
            id,
            entities: vec![],
            exec,
            listeners: vec![],
        }
    }
}

#[async_trait::async_trait]
impl Executor<Signal<rpc::worker::Request>, Signal<rpc::worker::Response>> for Worker {
    async fn execute(
        &self,
        sig: Signal<rpc::worker::Request>,
    ) -> Result<Signal<rpc::worker::Response>> {
        match &self.exec {
            WorkerExec::Remote(remote_exec) => {
                remote_exec
                    .execute(
                        Signal::from(sig)
                            .originating_at(Participant::Leader.into())
                            .into(),
                    )
                    .await?
            }
            WorkerExec::Local(local_exec) => {
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
