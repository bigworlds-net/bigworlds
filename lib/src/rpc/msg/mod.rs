//! Protocol for communications between clients and servers.

pub mod client_server;

pub use client_server::*;

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::convert::{TryFrom, TryInto};
use std::io;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;

use fnv::FnvHashMap;
use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};
use serde_bytes::serialize;
use serde_repr::*;
use uuid::Uuid;

use crate::error::Error;
use crate::net::Encoding;
use crate::query::{Query, QueryProduct, Trigger};
use crate::{EntityId, EntityName, Float, Int, Result, Snapshot, Var};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Message {
    OK,

    Disconnect,

    ErrorResponse(String),

    PingRequest(Vec<u8>),
    PingResponse(Vec<u8>),

    EntityListRequest,
    EntityListResponse(Vec<EntityName>),

    RegisterClientRequest(RegisterClientRequest),
    RegisterClientResponse(RegisterClientResponse),

    StatusRequest(StatusRequest),
    StatusResponse(StatusResponse),

    AdvanceRequest(AdvanceRequest),
    AdvanceResponse(AdvanceResponse),

    QueryRequest(Query),
    QueryResponse(QueryProduct),

    SubscribeRequest(Vec<Trigger>, Query),
    SubscribeResponse(Uuid),
    UnsubscribeRequest(Uuid),

    SpawnEntitiesRequest(SpawnEntitiesRequest),
    SpawnEntitiesResponse(SpawnEntitiesResponse),

    DataPullRequest(DataPullRequest),
    DataPullResponse(DataPullResponse),
    TypedDataPullRequest(TypedDataPullRequest),
    TypedDataPullResponse(TypedDataPullResponse),

    // DataTransferRequest(DataTransferRequest),
    // DataTransferResponse(DataTransferResponse),
    // TypedDataTransferRequest(TypedDataTransferRequest),
    // TypedDataTransferResponse(TypedDataTransferResponse),
    // ScheduledDataTransferRequest(ScheduledDataTransferRequest),
    ExportSnapshotRequest(ExportSnapshotRequest),
    ExportSnapshotResponse(ExportSnapshotResponse),

    /// Upload a project as tarball. Server must explicitly enable this option
    /// as it's disabled by default.
    UploadProjectArchiveRequest(UploadProjectRequest),
    UploadProjectArchiveResponse(UploadProjectResponse),

    ListScenariosRequest(ListScenariosRequest),
    ListScenariosResponse(ListScenariosResponse),
    LoadScenarioRequest(LoadScenarioRequest),
    LoadScenarioResponse(LoadScenarioResponse),

    InitializeRequest,
    InitializeResponse,

    InvokeRequest(InvokeRequest),

    SnapshotRequest,
    SnapshotResponse(SnapshotResponse),
}

impl Message {
    /// Deserializes from bytes.
    pub fn from_bytes(mut bytes: Vec<u8>, encoding: Encoding) -> Result<Message> {
        let msg = crate::util::decode(&bytes, encoding)?;
        Ok(msg)
    }

    /// Serializes into bytes.
    pub fn to_bytes(&self, encoding: Encoding) -> Result<Vec<u8>> {
        Ok(crate::util::encode(&self, encoding)?)
    }

    pub fn ok(&self) -> Result<()> {
        match self {
            Message::OK => Ok(()),
            Message::ErrorResponse(e) => Err(Error::ErrorResponse(e.to_owned())),
            _ => Err(Error::UnexpectedResponse(format!("{:?}", self))),
        }
    }
}

impl TryFrom<Message> for Snapshot {
    type Error = Error;
    fn try_from(msg: Message) -> std::result::Result<Self, Self::Error> {
        match msg {
            Message::SnapshotResponse(response) => Ok(response.snapshot),
            _ => Err(Error::UnexpectedResponse(format!("{:?}", msg))),
        }
    }
}
