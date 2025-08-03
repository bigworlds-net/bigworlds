use async_trait::async_trait;
use futures::Stream;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::client::Config;
use crate::net::CompositeAddress;
use crate::query::{self, Query, QueryProduct};
use crate::rpc::msg::{Message, StatusResponse};
use crate::{executor, EntityName, Model, PrefabName, Result};

#[async_trait]
pub trait AsyncClient {
    type Client;

    async fn connect(addr: CompositeAddress, config: Config) -> Result<Self::Client>;
    async fn shutdown(&mut self) -> Result<()>;
    async fn is_alive(&mut self) -> Result<()>;

    async fn initialize(&mut self) -> Result<()>;
    async fn spawn_entity(&mut self, name: EntityName, prefab: Option<PrefabName>) -> Result<()>;
    async fn despawn_entity(&mut self, name: &str) -> Result<()>;

    async fn get_model(&mut self) -> Result<Model>;
    async fn status(&mut self) -> Result<StatusResponse>;
    async fn step(&mut self, step_count: u32) -> Result<()>;
    async fn query(&mut self, q: Query) -> Result<QueryProduct>;

    async fn subscribe(
        &mut self,
        triggers: Vec<query::Trigger>,
        query: Query,
    ) -> Result<executor::Receiver<Message>>;
    async fn unsubscribe(&mut self, subscription_id: Uuid) -> Result<()>;
}
