use crate::{
    rpc::{
        self,
        worker::{Request, RequestLocal, Response},
    },
    Error, Executor, LocalExec, RemoteExec, Result, Signal,
};

pub type WorkerRemoteExec = RemoteExec<Signal<Request>, Result<Signal<Response>>>;
pub type WorkerLocalExec = LocalExec<Signal<RequestLocal>, Result<Signal<Response>>>;

#[derive(Clone)]
pub enum WorkerExec {
    /// Remote executor for sending requests to worker over the wire.
    Remote(WorkerRemoteExec),
    /// Local executor for sending requests to worker within the same runtime.
    Local(WorkerLocalExec),
}

#[async_trait::async_trait]
impl Executor<Signal<rpc::worker::Request>, Signal<rpc::worker::Response>> for WorkerExec {
    async fn execute(
        &self,
        sig: Signal<rpc::worker::Request>,
    ) -> Result<Signal<rpc::worker::Response>> {
        match self {
            WorkerExec::Remote(remote_exec) => remote_exec.execute(sig).await?,
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

impl std::fmt::Debug for WorkerExec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("worker executor: ");
        match self {
            Self::Remote(r) => {
                f.write_str("remote at: ")?;
                write!(f, "{}", r.remote_address());
            }
            Self::Local(_) => {
                f.write_str("local")?;
            }
        }
        Ok(())
    }
}
