use std::time::Duration;

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, WebSocket},
    MaybeTlsStream, WebSocketStream,
};
use uuid::Uuid;

use crate::{
    net::CompositeAddress,
    query,
    rpc::msg::{self, Message},
    EntityName, Error, PrefabName, Result, Snapshot,
};

use super::{AsyncClient, Config};

pub struct WsClient {
    #[allow(dead_code)]
    config: Config,
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl WsClient {
    pub async fn connect_direct() -> Result<Self> {
        let (mut socket, _) = connect_async("ws://127.0.0.1:15000/ws").await.unwrap();
        // let (tx, rx) = socket.split();

        let msg = Message::RegisterClientRequest(msg::RegisterClientRequest {
            name: "ws_client".to_owned(),
            is_blocking: false,
            auth_token: None,
            encodings: vec![],
            transports: vec![],
        });
        let msg = tungstenite::Message::Binary(bincode::serialize(&msg).unwrap());
        socket.send(msg).await;

        let resp = socket.next().await.unwrap().unwrap();

        Ok(Self {
            config: Config::default(),
            socket,
        })
    }

    pub async fn execute(&mut self, msg: Message) -> Result<Message> {
        let msg = tungstenite::Message::Binary(bincode::serialize(&msg)?);
        // println!("ws: sending for execution: {msg:?}");
        self.socket
            .send(msg)
            .await
            .map_err(|e| Error::WsNetworkError(e.to_string()))?;
        // println!("ws: sent");
        let resp = self.socket.next().await.unwrap().unwrap();
        // println!("ws: got response: {resp:?}");
        if let tungstenite::Message::Binary(msg) = resp {
            let resp: Message = bincode::deserialize(&msg)?;
            Ok(resp)
        } else {
            unimplemented!()
        }
    }
}

#[async_trait]
impl AsyncClient for WsClient {
    type Client = Self;

    async fn connect(addr: CompositeAddress, config: Config) -> Result<Self::Client> {
        println!("connecting at: {}", addr.to_string());
        let (mut socket, _) = connect_async(addr.to_string()).await.unwrap();
        // let (tx, rx) = socket.split();

        let msg = Message::RegisterClientRequest(msg::RegisterClientRequest {
            name: config.name.clone(),
            is_blocking: config.is_blocking,
            auth_token: None,
            encodings: vec![],
            transports: vec![],
        });
        let msg = tungstenite::Message::Binary(bincode::serialize(&msg).unwrap());
        socket.send(msg).await;

        let resp = socket.next().await.unwrap().unwrap();

        Ok(Self {
            config: Config::default(),
            socket,
        })
    }

    async fn shutdown(&mut self) -> Result<()> {
        info!("Disconnecting client...");
        self.execute(Message::Disconnect).await?;
        tokio::time::sleep(Duration::from_secs(1)).await;
        Ok(())
    }

    async fn is_alive(&mut self) -> Result<()> {
        self.execute(Message::PingRequest(vec![0])).await?;
        Ok(())
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

    async fn get_model(&mut self) -> Result<crate::Model> {
        todo!()
    }

    async fn status(&mut self) -> Result<crate::rpc::msg::StatusResponse> {
        let resp = self
            .execute(Message::StatusRequest(msg::StatusRequest {
                format: format!(""),
            }))
            .await?;
        // println!("ws: status response: {resp:?}");
        if let Message::StatusResponse(status) = resp {
            Ok(status)
        } else {
            Err(Error::UnexpectedResponse(format!("{resp:?}")))
        }
    }

    async fn step(&mut self, step_count: u32) -> Result<()> {
        self.execute(Message::AdvanceRequest(msg::AdvanceRequest {
            step_count,
            wait: true,
        }))
        .await?;
        Ok(())
    }

    async fn invoke(&mut self, event: &str) -> Result<()> {
        unimplemented!()
    }

    async fn query(&mut self, q: crate::Query) -> Result<crate::QueryProduct> {
        let resp = self.execute(Message::QueryRequest(q)).await?;
        if let Message::QueryResponse(product) = resp {
            Ok(product)
        } else {
            Err(Error::UnexpectedResponse(format!("{resp:?}")))
        }
    }

    async fn subscribe(
        &mut self,
        triggers: Vec<query::Trigger>,
        q: query::Query,
    ) -> Result<crate::executor::Receiver<Message>> {
        todo!()
    }
    async fn unsubscribe(&mut self, subscription_id: Uuid) -> Result<()> {
        todo!()
    }

    async fn snapshot(&mut self) -> Result<Snapshot> {
        todo!()
    }
}
