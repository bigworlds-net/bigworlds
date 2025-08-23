use crate::{
    rpc::{
        self,
        leader::{Request, RequestLocal, Response},
    },
    Error, Executor, LocalExec, RemoteExec, Result, Signal,
};

pub type LeaderRemoteExec = RemoteExec<Signal<Request>, Result<Signal<Response>>>;

#[derive(Clone)]
pub enum LeaderExec {
    /// Remote executor for sending requests to leader over the wire.
    Remote(LeaderRemoteExec),
    /// Local executor for sending requests to leader within the same runtime.
    Local(LocalExec<Signal<RequestLocal>, Result<Signal<Response>>>),
}

#[async_trait::async_trait]
impl Executor<Signal<rpc::leader::Request>, Signal<rpc::leader::Response>> for LeaderExec {
    async fn execute(
        &self,
        sig: Signal<rpc::leader::Request>,
    ) -> Result<Signal<rpc::leader::Response>> {
        match self {
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

impl std::fmt::Debug for LeaderExec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("leader executor: ");
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
