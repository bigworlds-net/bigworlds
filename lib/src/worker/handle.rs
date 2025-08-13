use uuid::Uuid;

use crate::{
    leader,
    rpc::{
        self,
        worker::{Request, RequestLocal, Response},
        Caller, Participant,
    },
    server, EntityName, Error, Executor, LocalExec, Model, Query, QueryProduct, Result, Signal,
};

#[derive(Clone)]
pub struct Handle {
    /// Controller executor, allowing control over the worker task.
    pub ctl: LocalExec<Signal<RequestLocal>, Result<Signal<Response>>>,

    /// Server executor for running requests coming from a local server.
    pub server_exec: LocalExec<Signal<RequestLocal>, Result<Signal<Response>>>,

    /// Executor for running requests coming from a local leader.
    pub leader_exec: LocalExec<Signal<RequestLocal>, Result<Signal<Response>>>,

    pub behavior_exec: LocalExec<Signal<Request>, Result<Signal<Response>>>,
    pub behavior_broadcast: tokio::sync::broadcast::Sender<rpc::behavior::Request>,
}

impl Handle {
    /// Connect the worker to another remote worker over a network using the
    /// provided address.
    pub async fn connect_to_worker(&self, address: &str) -> Result<()> {
        self.ctl
            .execute(Signal::from(
                rpc::worker::Request::ConnectToWorker(address.parse()?).into_local(),
            ))
            .await??;
        Ok(())
    }

    /// Connect the worker to a remote leader over a network using the provided
    /// address.
    pub async fn connect_to_leader(&self, address: &str) -> Result<()> {
        self.ctl
            .execute(Signal::from(
                rpc::worker::Request::ConnectToLeader(address.parse()?).into_local(),
            ))
            .await??;
        Ok(())
    }

    /// Connect the worker to a local leader task using the provided handle.
    pub async fn connect_to_local_leader(&self, handle: &leader::Handle) -> Result<()> {
        self.ctl
            .execute(Signal::from(RequestLocal::ConnectToLeader(
                handle.worker_exec.clone(),
                self.leader_exec.clone(),
            )))
            .await??;

        Ok(())
    }

    /// Connect the worker to another worker running on the same runtime.
    pub async fn connect_to_local_worker(&self, worker_handle: &Handle) -> Result<()> {
        self.ctl
            .execute(Signal::from(RequestLocal::ConnectToWorker()))
            .await??;

        Ok(())
    }

    /// Connect the worker a server running on the same runtime.
    pub async fn connect_to_local_server(&self, handle: &server::Handle) -> Result<()> {
        self.ctl
            .execute(Signal::from(RequestLocal::ConnectToServer(
                handle.worker.clone(),
                self.server_exec.clone(),
            )))
            .await??;

        Ok(())
    }

    pub async fn entities(&self) -> Result<Vec<EntityName>> {
        match self
            .ctl
            .execute(Signal::from(RequestLocal::Request(Request::EntityList)))
            .await??
            .into_payload()
        {
            Response::EntityList(list) => Ok(list),
            _ => unimplemented!(),
        }
    }

    pub async fn model(&self) -> Result<Model> {
        match self
            .ctl
            .execute(Signal::from(RequestLocal::Request(Request::GetModel)))
            .await??
            .into_payload()
        {
            Response::GetModel(model) => Ok(model),
            resp => Err(Error::UnexpectedResponse(resp.to_string())),
        }
    }

    pub async fn query(&self, query: Query, caller: Option<Caller>) -> Result<QueryProduct> {
        let caller = match caller {
            Some(c) => c,
            None => Participant::Worker(self.id().await?).into(),
        };

        match self
            .ctl
            .execute(
                Signal::from(RequestLocal::Request(Request::ProcessQuery(query)))
                    .originating_at(caller),
            )
            .await??
            .into_payload()
        {
            Response::Query(product) => Ok(product),
            resp => Err(Error::UnexpectedResponse(resp.to_string())),
        }
    }

    pub async fn id(&self) -> Result<Uuid> {
        match self
            .ctl
            .execute(Signal::from(RequestLocal::Request(Request::Status)))
            .await??
            .into_payload()
        {
            Response::Status {
                id,
                uptime,
                worker_count,
            } => Ok(id),
            _ => unimplemented!(),
        }
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.ctl
            .execute(Signal::from(RequestLocal::Shutdown))
            .await??;
        Ok(())
    }
}
