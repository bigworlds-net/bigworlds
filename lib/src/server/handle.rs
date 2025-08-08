use crate::executor::{Executor, LocalExec, Signal};
use crate::rpc;
use crate::rpc::msg::Message;
use crate::server::{worker, ClientId};
use crate::worker::WorkerId;
use crate::Result;

#[derive(Clone)]
pub struct Handle {
    pub ctl: LocalExec<Signal<rpc::server::RequestLocal>, Result<Signal<rpc::server::Response>>>,

    pub client: LocalExec<(Option<ClientId>, rpc::msg::Message), rpc::msg::Message>,
    pub client_id: Option<ClientId>,

    pub worker: LocalExec<Signal<rpc::server::RequestLocal>, Result<Signal<rpc::server::Response>>>,
    pub worker_id: Option<WorkerId>,
    // TODO: return a list of listeners that were successfully established.
    // pub listeners: Vec<CompositeAddress>,
}

#[async_trait::async_trait]
impl Executor<Message, Message> for Handle {
    async fn execute(&self, msg: Message) -> Result<Message> {
        self.client
            .execute((self.client_id, msg))
            .await
            .map_err(|e| e.into())
    }
}

impl Handle {
    /// Connects server to worker.
    pub async fn connect_to_worker(
        &mut self,
        worker_handle: &worker::Handle,
        duplex: bool,
    ) -> Result<()> {
        let mut _server_id = None;

        // Connect server to worker.
        if let rpc::server::Response::ConnectToWorker { server_id } = self
            .ctl
            .execute(Signal::from(rpc::server::RequestLocal::ConnectToWorker(
                worker_handle.server_exec.clone(),
                self.worker.clone(),
            )))
            .await??
            .into_payload()
        {
            _server_id = Some(server_id);
        }

        if duplex {
            worker_handle.connect_to_local_server(&self).await?;
        }

        Ok(())
    }
}
