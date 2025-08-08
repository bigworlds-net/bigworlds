use crate::{
    rpc::leader::{Request, RequestLocal, Response},
    LocalExec, RemoteExec, Result, Signal,
};

pub type LeaderRemoteExec = RemoteExec<Signal<Request>, Result<Signal<Response>>>;

#[derive(Clone)]
pub enum LeaderExec {
    /// Remote executor for sending requests to leader over the wire.
    Remote(LeaderRemoteExec),
    /// Local executor for sending requests to leader within the same runtime.
    Local(LocalExec<Signal<RequestLocal>, Result<Signal<Response>>>),
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
