use crate::{
    rpc::node::{Request, Response},
    RemoteExec, Result,
};

#[derive(Clone)]
pub struct OtherNode {
    pub exec: RemoteExec<Request, Result<Response>>,
}
