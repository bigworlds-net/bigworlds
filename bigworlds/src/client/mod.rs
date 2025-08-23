pub mod r#async;

#[cfg(feature = "ws_transport")]
pub mod ws;

pub use r#async::AsyncClient;
#[cfg(feature = "ws_transport")]
pub use ws::WsClient;

use async_trait::async_trait;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

use crate::executor::ExecutorMulti;
use crate::net::{CompositeAddress, Encoding, Transport};
use crate::rpc::msg::{self, Message, PullRequestData};
use crate::time::Duration;
use crate::{
    query, Address, EntityName, Error, Model, PrefabName, RemoteExec, Result, Snapshot, Var,
};
use crate::{Executor, Query, QueryProduct};

/// Basic client based on the default remote executor interface based on the
/// quic transport.
///
/// Uses the standard `rpc::msg::Message`-based protocol.
#[derive(Clone)]
pub struct Client {
    config: Config,
    exec: RemoteExec<Message, Message>,
}

impl Client {
    pub async fn pull(&self, vars: Vec<(Address, Var)>) -> Result<()> {
        let msg = self
            .execute(Message::DataPullRequest(msg::DataPullRequest {
                data: PullRequestData::AddressedVars(vars),
            }))
            .await?;
        match msg {
            Message::DataPullResponse(response) => {
                if response.error.is_empty() {
                    Ok(())
                } else {
                    // Err(Error)
                    unimplemented!()
                }
            }
            // Message::ErrorResponse(e) => Err(Error::ErrorResponse(e)),
            m => Err(Error::UnexpectedResponse(format!(
                "expected DataPullResponse, got: {:?}",
                m
            ))),
        }
    }

    pub async fn execute(&self, msg: Message) -> Result<Message> {
        self.exec.execute(msg).await
    }
}

#[async_trait]
impl AsyncClient for Client {
    type Client = Client;

    async fn connect(addr: CompositeAddress, config: Config) -> Result<Self> {
        let connection = crate::net::quic::make_connection(addr.address.try_into()?).await?;
        let exec = RemoteExec::<Message, Message>::new(connection);

        let resp = exec
            .execute(Message::RegisterClientRequest(msg::RegisterClientRequest {
                name: config.name.clone(),
                is_blocking: config.is_blocking,
                auth_token: None,
                encodings: vec![],
                transports: vec![],
            }))
            .await?;

        Ok(Client { config, exec })
    }

    async fn shutdown(&mut self) -> Result<()> {
        info!("Disconnecting client...");
        self.execute(Message::Disconnect).await?;
        tokio::time::sleep(Duration::from_secs(1)).await;
        Ok(())
    }

    async fn is_alive(&mut self) -> Result<()> {
        match self.exec.close_reason() {
            Some(reason) => Err(Error::Other(format!(
                "connection closed: reason: {}",
                reason
            ))),
            None => Ok(()),
        }
    }

    async fn initialize(&mut self) -> Result<()> {
        todo!()
    }
    async fn spawn_entity(&mut self, name: EntityName, prefab: Option<PrefabName>) -> Result<()> {
        todo!()
    }
    async fn despawn_entity(&mut self, name: &str) -> Result<()> {
        todo!()
    }

    async fn get_model(&mut self) -> Result<Model> {
        todo!()
    }
    async fn status(&mut self) -> Result<msg::StatusResponse> {
        let resp = self
            .execute(Message::StatusRequest(msg::StatusRequest {
                format: format!(""),
            }))
            .await?;
        if let Message::StatusResponse(status) = resp {
            Ok(status)
        } else {
            Err(Error::UnexpectedResponse(format!("{resp:?}")))
        }
    }

    async fn step(&mut self, step_count: u32) -> Result<()> {
        if let Message::ErrorResponse(e) = self
            .execute(Message::AdvanceRequest(msg::AdvanceRequest {
                step_count,
                wait: true,
            }))
            .await?
        {
            Err(Error::ErrorResponse(e))
        } else {
            Ok(())
        }
    }

    async fn invoke(&mut self, event: &str) -> Result<()> {
        if let Message::ErrorResponse(e) = self
            .execute(
                msg::InvokeRequest {
                    event: event.to_owned(),
                }
                .into(),
            )
            .await?
        {
            Err(Error::ErrorResponse(e))
        } else {
            Ok(())
        }
    }

    async fn query(&mut self, query: Query) -> Result<QueryProduct> {
        let msg = self.execute(Message::QueryRequest(query)).await?;
        match msg {
            Message::QueryResponse(product) => Ok(product),
            Message::ErrorResponse(e) => Err(Error::ErrorResponse(e)),
            m => Err(Error::UnexpectedResponse(format!(
                "expected QueryResponse, got: {:?}",
                m
            ))),
        }
    }

    async fn subscribe(
        &mut self,
        triggers: Vec<query::Trigger>,
        query: Query,
    ) -> Result<crate::executor::Receiver<Message>> {
        Ok(self
            .exec
            .execute_to_multi(Message::SubscribeRequest(triggers, query))
            .await?
            .into())
    }

    async fn unsubscribe(&mut self, subscription_id: Uuid) -> Result<()> {
        self.exec
            .execute(Message::UnsubscribeRequest(subscription_id))
            .await?
            .ok()
    }

    async fn snapshot(&mut self) -> Result<Snapshot> {
        self.execute(Message::SnapshotRequest).await?.try_into()
    }
}

/// Configuration settings for client.
#[derive(Debug, Clone)]
pub struct Config {
    /// Self-assigned name.
    pub name: String,
    /// Whether to send regular heartbeats, and with what frequency.
    pub heartbeat_interval: Option<Duration>,
    /// Blocking client requires server to wait for it's explicit step advance.
    pub is_blocking: bool,
    /// Compression policy for outgoing messages.
    pub compress: CompressionPolicy,
    /// Supported encodings, first is most preferred.
    pub encodings: Vec<Encoding>,
    /// Supported transports, first is most preferred.
    pub transports: Vec<Transport>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            heartbeat_interval: Some(Duration::from_secs(1)),
            is_blocking: false,
            compress: CompressionPolicy::Nothing,
            encodings: vec![Encoding::Bincode],
            transports: vec![Transport::Quic],
        }
    }
}

/// List of available compression policies for outgoing messages.
#[derive(Debug, Clone)]
pub enum CompressionPolicy {
    /// Compress all outgoing traffic.
    Everything,
    /// Only compress messages larger than given size in bytes.
    LargerThan(usize),
    /// Only compress data-heavy query results.
    OnlyQueryProducts,
    /// Don't use compression.
    Nothing,
}

impl CompressionPolicy {
    pub fn from_str(s: &str) -> Result<Self> {
        if s.starts_with("bigger_than_") || s.starts_with("larger_than_") {
            let split = s.split('_').collect::<Vec<&str>>();
            let number = split[2]
                .parse::<usize>()
                .map_err(|e| Error::Other(e.to_string()))?;
            return Ok(Self::LargerThan(number));
        }
        let c = match s {
            "all" | "everything" => Self::Everything,
            "data" | "only_data" => Self::OnlyQueryProducts,
            "none" | "nothing" => Self::Nothing,
            _ => {
                return Err(Error::Other(format!(
                    "failed parsing compression policy from string: {}",
                    s
                )))
            }
        };
        Ok(c)
    }
}
