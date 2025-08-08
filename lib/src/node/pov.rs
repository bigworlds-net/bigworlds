use crate::{
    rpc::node::{Request, Response},
    RemoteExec, Result,
};

#[derive(Clone)]
pub struct RemoteNode {
    pub exec: RemoteExec<Request, Result<Response>>,
}
