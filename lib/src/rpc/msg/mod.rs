//! Message definitions.

#![allow(unused)]

pub mod client_server;

pub use client_server::*;
use uuid::Uuid;

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

use crate::error::Error;
use crate::net::Encoding;
use crate::query::{Query, QueryProduct, Trigger};
use crate::{EntityId, EntityName, Float, Int, Result, Var};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
}

impl Message {
    /// Deserializes from bytes.
    pub fn from_bytes(mut bytes: Vec<u8>, encoding: &Encoding) -> Result<Message> {
        let msg = decode(&bytes, encoding)?;
        Ok(msg)
    }

    /// Serializes into bytes.
    pub fn to_bytes(&self, encoding: Encoding) -> Result<Vec<u8>> {
        Ok(crate::util_net::encode(&self, encoding)?)
    }

    pub fn ok(&self) -> Result<()> {
        match self {
            Message::OK => Ok(()),
            Message::ErrorResponse(e) => Err(Error::ErrorResponse(e.to_owned())),
            _ => Err(Error::UnexpectedResponse(format!("{:?}", self))),
        }
    }
}

/// Unpacks object from bytes based on selected encoding.
pub fn decode<'de, P: Deserialize<'de>>(bytes: &'de [u8], encoding: &Encoding) -> Result<P> {
    let unpacked = match encoding {
        Encoding::Bincode => bincode::deserialize(bytes)?,
        Encoding::MsgPack => {
            #[cfg(not(feature = "msgpack_encoding"))]
            panic!("trying to unpack using msgpack encoding, but msgpack_encoding crate feature is not enabled");
            #[cfg(feature = "msgpack_encoding")]
            {
                use rmp_serde::config::StructMapConfig;
                let mut de = rmp_serde::Deserializer::new(bytes).with_binary();
                Deserialize::deserialize(&mut de)?
            }
        }
        Encoding::Json => {
            #[cfg(not(feature = "json_encoding"))]
            panic!("trying to unpack using json encoding, but json_encoding crate feature is not enabled");
            #[cfg(feature = "json_encoding")]
            {
                serde_json::from_slice(bytes)?
            }
        }
    };
    Ok(unpacked)
}
