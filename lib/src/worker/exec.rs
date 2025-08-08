use crate::{
    rpc::worker::{Request, RequestLocal, Response},
    LocalExec, RemoteExec, Result, Signal,
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
