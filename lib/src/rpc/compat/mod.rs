//! "Compatibility" msg interface definitions, supposed to work with different
//! programming languages and encoding schemes.
//!
//! This interface is the one compatible with a wider range of encoding
//! schemes and programming languages, as compared to the one defined in
//! `bigworlds-rs::msg`. For example, the latter makes heavy use of
//! Rust enums, which proved problematic with schemes like `msgpack`.

#![allow(unused)]

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

pub mod server_client;
pub mod transforms;

pub use server_client::*;

use crate::net::Encoding;
use crate::util_net::{decode, encode};
use crate::{Float, Int, Result, Var};

/// Enumeration of all available message types.
#[derive(Debug, Clone, Copy, PartialEq, TryFromPrimitive, Deserialize_repr, Serialize_repr)]
#[repr(u8)]
pub enum MessageType {
    PingRequest,
    PingResponse,

    RegisterClientRequest,
    RegisterClientResponse,

    ListLocalScenariosRequest,
    ListLocalScenariosResponse,
    LoadLocalScenarioRequest,
    LoadLocalScenarioResponse,
    LoadRemoteScenarioRequest,
    LoadRemoteScenarioResponse,

    IntroduceLeaderRequest,
    IntroduceLeaderResponse,
    IntroduceWorkerToLeaderRequest,
    IntroduceWorkerToLeaderResponse,

    ExportSnapshotRequest,
    ExportSnapshotResponse,

    RegisterRequest,
    RegisterResponse,

    StatusRequest,
    StatusResponse,

    NativeQueryRequest,
    NativeQueryResponse,
    QueryRequest,
    QueryResponse,

    DataTransferRequest,
    DataTransferResponse,
    TypedDataTransferRequest,
    TypedDataTransferResponse,

    JsonPullRequest,
    JsonPullResponse,
    DataPullRequest,
    DataPullResponse,
    TypedDataPullRequest,
    TypedDataPullResponse,

    ScheduledDataTransferRequest,
    ScheduledDataTransferResponse,

    TurnAdvanceRequest,
    TurnAdvanceResponse,

    SpawnEntitiesRequest,
    SpawnEntitiesResponse,

    StepAdvanceRequest,
    StepAdvanceResponse,
}

/// Self-described message structure wrapping a byte payload.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Message {
    /// Describes what is stored within the payload
    pub type_: MessageType,
    /// Byte representation of the message payload
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
}

impl Message {
    /// Creates a complete `Message` from a payload struct.
    pub fn from_payload<P>(payload: P, encoding: Encoding) -> Result<Message>
    where
        P: Clone + Serialize + Payload,
    {
        let msg_type = payload.r#type();
        let bytes = encode(payload, encoding).unwrap();
        Ok(Message {
            type_: msg_type,
            payload: bytes,
        })
    }

    /// Deserializes from bytes.
    pub fn from_bytes(mut bytes: Vec<u8>, encoding: Encoding) -> Result<Message> {
        let msg = decode(&bytes, encoding).unwrap();
        Ok(msg)
    }

    /// Serializes into bytes.
    pub fn to_bytes(mut self, encoding: Encoding) -> Result<Vec<u8>> {
        Ok(encode(self, encoding).unwrap())
    }

    /// Unpacks message payload into a payload struct of provided type.
    pub fn unpack_payload<'de, P: Payload + Deserialize<'de>>(
        &'de self,
        encoding: Encoding,
    ) -> Result<P> {
        let unpacked = decode(&self.payload, encoding).unwrap();
        Ok(unpacked)
    }
}

pub trait Payload: Clone {
    /// Allows payload message structs to state their message type.
    fn r#type(&self) -> MessageType;
}

/// Version of the `Var` struct used for untagged ser/deser.
#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VarJson {
    String(String),
    Int(Int),
    Float(Float),
    Bool(bool),
    Byte(u8),
    List(Vec<VarJson>),
    Grid(Vec<Vec<VarJson>>),
    Map(BTreeMap<VarJson, VarJson>),
}

impl Eq for VarJson {}

impl Ord for VarJson {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Equal)
    }
}

impl From<Var> for VarJson {
    fn from(var: Var) -> Self {
        match var {
            // Var::String(v) => VarJson::String(v),
            Var::Int(v) => VarJson::Int(v),
            Var::Float(v) => VarJson::Float(v),
            Var::Bool(v) => VarJson::Bool(v),
            Var::Byte(v) => VarJson::Byte(v),
            _ => unimplemented!(),
        }
    }
}
impl Into<Var> for VarJson {
    fn into(self) -> Var {
        match self {
            // VarJson::String(v) => Var::String(v),
            VarJson::Int(v) => Var::Int(v),
            VarJson::Float(v) => Var::Float(v),
            VarJson::Bool(v) => Var::Bool(v),
            VarJson::Byte(v) => Var::Byte(v),
            _ => unimplemented!(),
        }
    }
}
