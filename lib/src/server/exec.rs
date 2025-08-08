use crate::{
    rpc::server::{Request, RequestLocal, Response},
    LocalExec, RemoteExec, Result, Signal,
};

/// Servers are attached to workers to handle distributing of data to
/// clients.
///
/// Worker tracks associated servers. Worker can send requests to connected
/// servers, for example telling them to reconnect to different worker.
#[derive(Clone)]
pub enum ServerExec {
    /// Remote executor for sending requests to a server over the wire.
    Remote(RemoteExec<Signal<Request>, Result<Signal<Response>>>),
    /// Local executor for sending requests to a server within the same
    /// runtime.
    Local(LocalExec<Signal<RequestLocal>, Result<Signal<Response>>>),
}
