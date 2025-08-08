use std::net::SocketAddr;

use tokio::sync::watch;

use crate::executor::{self, ExecutorMulti};
use crate::net::Encoding;
use crate::rpc::Participant;
use crate::server::{ClientId, ServerId};
use crate::time::{Duration, Instant};
use crate::worker::WorkerExec;
use crate::{rpc, Error, Executor, Result, Signal};

/// Connected client as seen by the server.
#[derive(Clone, Debug)]
pub struct Client {
    /// Id assigned by the server.
    pub id: ClientId,
    /// Self-assigned name.
    pub name: String,

    /// IP address of the client.
    pub addr: Option<SocketAddr>,

    /// Currently applied encoding as negotiated with the client.
    pub encoding: Encoding,

    /// Blocking client has to explicitly agree to let server continue stepping
    /// forward, while non-blocking client is more of a passive observer.
    pub is_blocking: bool,
    /// Watch channel for specifying blocking conditions for the client.
    /// Specifically it defines until what clock value the client is allowing
    /// execution. `None` means the client is blocked.
    pub unblocked_until: (watch::Sender<Option<usize>>, watch::Receiver<Option<usize>>),

    /// Client-specific keepalive value, if none server config value applies.
    pub keepalive: Option<Duration>,

    /// Auth token provided by the client.
    pub auth_token: Option<String>,

    /// Time of the last
    pub last_request: Instant,
}

// Connected worker as seen by the server.
#[derive(Clone)]
pub struct Worker {
    pub exec: WorkerExec,
    /// Unique self-assigned id, used for authenticating with worker.
    pub server_id: ServerId,
}

#[async_trait::async_trait]
impl Executor<Signal<rpc::worker::Request>, Signal<rpc::worker::Response>> for Worker {
    async fn execute(
        &self,
        sig: Signal<rpc::worker::Request>,
    ) -> Result<Signal<rpc::worker::Response>> {
        match &self.exec {
            WorkerExec::Remote(remote_exec) => {
                remote_exec
                    .execute(sig.originating_at(Participant::Server(self.server_id).into()))
                    .await?
            }
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

#[async_trait::async_trait]
impl ExecutorMulti<Signal<rpc::worker::Request>, Result<Signal<rpc::worker::Response>>> for Worker {
    async fn execute_to_multi(
        &self,
        sig: Signal<rpc::worker::Request>,
    ) -> Result<executor::Receiver<Result<Signal<rpc::worker::Response>>>> {
        match &self.exec {
            WorkerExec::Remote(remote_exec) => {
                remote_exec
                    .execute_to_multi(
                        sig.originating_at(Participant::Server(self.server_id).into()),
                    )
                    .await
            }
            WorkerExec::Local(local_exec) => {
                let sig = Signal::new(sig.payload.into(), sig.ctx);
                local_exec.execute_to_multi(sig).await
            }
        }
    }
}
