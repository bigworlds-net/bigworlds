use crate::{
    rpc::node::{Request, Response},
    Executor, LocalExec, Result,
};

use super::Config;

#[derive(Clone)]
pub struct Handle {
    pub config: Config,
    pub ctl: LocalExec<Request, Result<Response>>,
}

#[async_trait::async_trait]
impl Executor<Request, Response> for Handle {
    async fn execute(&self, req: Request) -> Result<Response> {
        self.ctl.execute(req).await?.map_err(|e| e.into())
    }
}
