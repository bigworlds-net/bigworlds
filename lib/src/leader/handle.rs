use crate::{
    net::CompositeAddress,
    rpc::leader::{Request, RequestLocal, Response},
    worker, Executor, LocalExec, Result, Signal,
};

/// Execution handle for sending requests to leader.
///
/// # Worker identification
///
/// The executor implementation for this type must send a `worker_id` to
/// identify with leader. This type stores a `worker_id` to use for
/// subsequent requests.
///
/// We can also supply any other `worker_id` but then we must go through
/// the `worker_exec` and not the executor implemented directly on
/// `LeaderHandle`.
#[derive(Clone)]
pub struct Handle {
    pub ctl: LocalExec<Signal<RequestLocal>, Result<Signal<Response>>>,

    /// Worker executor for running requests coming from a local worker
    pub worker_exec: LocalExec<Signal<RequestLocal>, Result<Signal<Response>>>,
    // pub worker_id: Option<WorkerId>,
}

#[async_trait::async_trait]
impl Executor<Signal<Request>, Result<Signal<Response>>> for Handle {
    async fn execute(&self, sig: Signal<Request>) -> Result<Result<Signal<Response>>> {
        let sig = Signal::new(sig.payload.into(), sig.ctx);
        self.ctl.execute(sig).await.map_err(|e| e.into())
    }
}

impl Handle {
    /// Connects leader to remote worker.
    pub async fn connect_to_remote_worker(&mut self, addr: CompositeAddress) -> Result<()> {
        let req: RequestLocal = Request::ConnectToWorker(addr).into();
        self.ctl.execute(Signal::from(req)).await??;
        Ok(())
    }

    /// Connects leader to worker existing on the same runtime.
    ///
    /// It also makes sure to connect worker to leader. Local channel
    /// communications are more difficult and we need to set things up both
    /// ways.
    pub async fn connect_to_local_worker(
        &mut self,
        worker_handle: &worker::Handle,
        duplex: bool,
    ) -> Result<()> {
        // Issue this leader to reach out with a connection to the worker.
        self.ctl
            .execute(Signal::from(RequestLocal::ConnectToWorker(
                worker_handle.leader_exec.clone(),
                self.worker_exec.clone(),
            )))
            .await??;

        if duplex {
            // Prompt the worker to connect to this leader as well.
            worker_handle.connect_to_local_leader(self).await?;
        }

        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.ctl
            .execute(Signal::from(RequestLocal::Shutdown))
            .await??;
        Ok(())
    }
}
