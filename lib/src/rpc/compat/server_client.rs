use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

use fnv::FnvHashMap;
use serde_repr::{Deserialize_repr, Serialize_repr};

use crate::net::{Encoding, Transport};
use crate::{
    Address, CompName, EntityId, EntityName, Float, Int, QueryProduct, Var, VarName, VarType,
};

use super::{MessageType, Payload};

/// Requests a simple `PingResponse` message. Can be used to check
/// the connection to the server.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct PingRequest {
    pub bytes: Vec<u8>,
}
pub(crate) const PING_REQUEST: &str = "PingRequest";
impl Payload for PingRequest {
    fn r#type(&self) -> MessageType {
        MessageType::PingRequest
    }
}

/// Response to `PingRequest` message.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct PingResponse {
    pub bytes: Vec<u8>,
}
impl Payload for PingResponse {
    fn r#type(&self) -> MessageType {
        MessageType::PingResponse
    }
}

/// Requests a few variables related to the current status of
/// the server.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct StatusRequest {
    pub format: String,
}
impl Payload for StatusRequest {
    fn r#type(&self) -> MessageType {
        MessageType::StatusRequest
    }
}

/// Response containing a few variables related to the current status of
/// the server.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct StatusResponse {
    pub name: String,
    pub description: String,
    // pub address: String,
    pub connected_clients: Vec<String>,
    pub engine_version: String,
    pub uptime: usize,
    pub current_tick: usize,

    pub scenario_name: String,
    pub scenario_title: String,
    pub scenario_desc: String,
    pub scenario_desc_long: String,
    pub scenario_author: String,
    pub scenario_website: String,
    pub scenario_version: String,
    pub scenario_engine: String,
    pub scenario_mods: Vec<String>,
    pub scenario_settings: Vec<String>,
}
impl Payload for StatusResponse {
    fn r#type(&self) -> MessageType {
        MessageType::StatusResponse
    }
}

/// Requests registration of the client who's sending the message.
/// This is the default first message any connecting client has to send
/// before sending anything else.
///
/// If successful the client is added to the server's list of registered
/// clients. Server will try to keep all connections with registered
/// clients alive.
///
/// `name` self assigned name of the client.
///
/// `is_blocking` specifies whether the client is a blocking client.
/// A blocking client is one that has to explicitly agree for the server to
/// start processing the next tick/turn.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RegisterClientRequest {
    pub name: String,
    pub is_blocking: bool,
    pub auth_pair: Option<(String, String)>,
    pub encodings: Vec<Encoding>,
    pub transports: Vec<Transport>,
}
impl Payload for RegisterClientRequest {
    fn r#type(&self) -> MessageType {
        MessageType::RegisterClientRequest
    }
}

/// Response to a `RegisterClientRequest` message.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct RegisterClientResponse {
    pub encoding: Encoding,
    pub transport: Transport,
    pub address: String,
}
impl Payload for RegisterClientResponse {
    fn r#type(&self) -> MessageType {
        MessageType::RegisterClientResponse
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct QueryRequest {
    pub query: Query,
}
impl Payload for QueryRequest {
    fn r#type(&self) -> MessageType {
        MessageType::QueryRequest
    }
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
            Var::String(v) => VarJson::String(v),
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
            VarJson::String(v) => Var::String(v),
            VarJson::Int(v) => Var::Int(v),
            VarJson::Float(v) => Var::Float(v),
            VarJson::Bool(v) => Var::Bool(v),
            VarJson::Byte(v) => Var::Byte(v),
            _ => unimplemented!(),
        }
    }
}

/// Alternative query structure compatible with environments that don't
/// support native query's variant enum layout.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Query {
    pub trigger: Trigger,
    pub description: Description,
    pub layout: Layout,
    pub filters: Vec<Filter>,
    pub mappings: Vec<Map>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Trigger {
    pub r#type: TriggerType,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum TriggerType {
    Immediate,
    Event,
    Mutation,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Filter {
    pub r#type: FilterType,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum FilterType {
    AllComponents,
    SomeComponents,
    Name,
    Id,
    VarRange,
    AttrRange,
    Distance,
    Node,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Map {
    pub r#type: MapType,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum MapType {
    All,
    SelectAddr,
    Components,
    Var,
    VarName,
    VarType,
}

#[derive(Clone, Debug, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum Layout {
    Var,
    Typed,
}

#[derive(Clone, Debug, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum Description {
    NativeDescribed,
    Addressed,
    StringAddressed,
    Ordered,
    None,
}

/// Requests one-time transfer of data from server to client.
///
/// `transfer_type` defines the process of data selection:
///     - `Full` get all the data from the sim database (ignores `selection`)
///     - `Selected` get some selected data, based on the `selection` list
///
/// `selection` is a list of addresses that can be used to select data
/// for transfer.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct DataTransferRequest {
    pub transfer_type: String,
    pub selection: Vec<String>,
}
impl Payload for DataTransferRequest {
    fn r#type(&self) -> MessageType {
        MessageType::DataTransferRequest
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
// #[serde(untagged)]
pub enum TransferResponseData {
    Typed(TypedSimDataPack),
    Var(VarSimDataPack),
    AddressedVar(FnvHashMap<Address, Var>),
    VarOrdered(u32, VarSimDataPackOrdered),
}

/// Response to `DataTransferRequest`.
///
/// `data` structure containing a set of lists containing different types of
/// data.
///
/// `error` contains the report of any errors that might have occurred.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DataTransferResponse {
    pub data: TransferResponseData,
}
impl Payload for DataTransferResponse {
    fn r#type(&self) -> MessageType {
        MessageType::DataTransferResponse
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ScheduledDataTransferRequest {
    pub event_triggers: Vec<String>,
    pub transfer_type: String,
    pub selection: Vec<String>,
}
impl Payload for ScheduledDataTransferRequest {
    fn r#type(&self) -> MessageType {
        MessageType::ScheduledDataTransferRequest
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct VarSimDataPackOrdered {
    pub vars: Vec<Var>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct VarSimDataPack {
    pub vars: FnvHashMap<(EntityName, CompName, VarName), Var>,
}

/// Structure holding all data organized based on data types.
///
/// Each data type is represented by a set of key-value pairs, where
/// keys are addresses represented with strings.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TypedSimDataPack {
    pub strings: HashMap<Address, String>,
    pub ints: HashMap<Address, Int>,
    pub floats: HashMap<Address, Float>,
    pub bools: HashMap<Address, bool>,
    pub string_lists: HashMap<Address, Vec<String>>,
    pub int_lists: HashMap<Address, Vec<Int>>,
    pub float_lists: HashMap<Address, Vec<Float>>,
    pub bool_lists: HashMap<Address, Vec<bool>>,
    pub string_grids: HashMap<Address, Vec<Vec<String>>>,
    pub int_grids: HashMap<Address, Vec<Vec<Int>>>,
    pub float_grids: HashMap<Address, Vec<Vec<Float>>>,
    pub bool_grids: HashMap<Address, Vec<Vec<bool>>>,
}
impl TypedSimDataPack {
    pub fn empty() -> TypedSimDataPack {
        TypedSimDataPack {
            strings: HashMap::new(),
            ints: HashMap::new(),
            floats: HashMap::new(),
            bools: HashMap::new(),
            string_lists: HashMap::new(),
            int_lists: HashMap::new(),
            float_lists: HashMap::new(),
            bool_lists: HashMap::new(),
            string_grids: HashMap::new(),
            int_grids: HashMap::new(),
            float_grids: HashMap::new(),
            bool_grids: HashMap::new(),
        }
    }
    pub fn from_query_product(qp: QueryProduct) -> Self {
        let mut data = Self::empty();
        match qp {
            QueryProduct::AddressedTyped(atm) => {
                for (fa, f) in atm.floats {
                    data.floats.insert(fa.into(), f);
                }
            }
            _ => (),
        }
        data
    }

    pub fn add(&mut self, addr: &Address, value_str: &str) {
        match addr.var_type {
            VarType::String => {
                self.strings.insert(addr.clone(), value_str.to_owned());
            }
            VarType::Int => {
                self.ints
                    .insert(addr.clone(), value_str.parse::<Int>().unwrap());
            }
            VarType::Float => {
                self.floats
                    .insert(addr.clone(), value_str.parse::<Float>().unwrap());
            }
            VarType::Bool => {
                self.bools
                    .insert(addr.clone(), value_str.parse::<bool>().unwrap());
            }
            _ => (),
        };
        ()
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct TypedDataTransferRequest {
    pub transfer_type: String,
    pub selection: Vec<String>,
}
impl Payload for TypedDataTransferRequest {
    fn r#type(&self) -> MessageType {
        MessageType::TypedDataTransferRequest
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TypedDataTransferResponse {
    pub data: TypedSimDataPack,
    pub error: String,
}
impl Payload for TypedDataTransferResponse {
    fn r#type(&self) -> MessageType {
        MessageType::TypedDataTransferResponse
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum PullRequestData {
    /// Request to pull a key-value map of addresses and vars in string-form
    Typed(TypedSimDataPack),
    // TypedOrdered(TypedSimDataPackOrdered),
    NativeAddressedVar((EntityId, CompName, VarName), Var),
    /// Request to pull a key-value map of addresses and serialized vars
    NativeAddressedVars(VarSimDataPack),
    AddressedVars(Vec<(Address, Var)>),
    /// Request to pull an ordered list of serialized vars, based on ordering
    /// provided by server when responding to data transfer request
    VarOrdered(u32, VarSimDataPackOrdered),
}

/// Request the server to pull provided data into the main simulation
/// database.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DataPullRequest {
    pub data: PullRequestData,
}
impl Payload for DataPullRequest {
    fn r#type(&self) -> MessageType {
        MessageType::DataPullRequest
    }
}

/// Response to `DataPullRequest`.
///
/// `error` contains the report of any errors that might have occurred.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct DataPullResponse {
    pub error: String,
}
impl Payload for DataPullResponse {
    fn r#type(&self) -> MessageType {
        MessageType::DataPullResponse
    }
}

/// Request the server to pull provided data into the main simulation
/// database.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TypedDataPullRequest {
    pub data: TypedSimDataPack,
}
impl Payload for TypedDataPullRequest {
    fn r#type(&self) -> MessageType {
        MessageType::TypedDataPullRequest
    }
}

/// Response to `DataPullRequest`.
///
/// `error` contains the report of any errors that might have occurred.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct TypedDataPullResponse {
    pub error: String,
}
impl Payload for TypedDataPullResponse {
    fn r#type(&self) -> MessageType {
        MessageType::TypedDataPullResponse
    }
}

/// Requests an advancement of the simulation by a turn, which the client
/// understands as a set number of simulation ticks. This number is
/// sent within the request.
///
/// In a situation with multiple blocking clients, this request acts
/// as a "thumbs up" signal from the client sending it. Until all
/// blocking clients have sent the signal that they are _ready_,
/// processing cannot continue.
///
/// `TurnAdvanceRequest` is only valid for clients that are _blocking_.
/// If the client has `is_blocking` option set to true then
/// the server will block processing every time it sends
/// a `TurnAdvanceResponse` to that client. If the client is not
/// blocking the server will ignore the request and the response to
/// this request will contain an error.
///
/// `tick_count` is the number of ticks the client considers _one turn_.
/// Server takes this value and sends a `TurnAdvanceResponse`
/// only after a number of ticks equal to the value of `tick_count`
/// is processed.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct StepAdvanceRequest {
    /// Number of steps to advance the simulation by
    pub step_count: u32,
    /// Require response to be sent only once once the request was fulfilled
    pub wait: bool,
}
impl Payload for StepAdvanceRequest {
    fn r#type(&self) -> MessageType {
        MessageType::StepAdvanceRequest
    }
}

/// Response to `TurnAdvanceRequest`.
///
/// `error` contains report of errors if any were encountered.
/// Possible errors include:
/// - `ClientIsNotBlocking`
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct TurnAdvanceResponse {
    pub error: String,
}
impl Payload for TurnAdvanceResponse {
    fn r#type(&self) -> MessageType {
        MessageType::TurnAdvanceResponse
    }
}

/// Requests the server to spawn a number of entities.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct SpawnEntitiesRequest {
    /// List of entity prefabs to be spawned as new entities
    pub entity_prefabs: Vec<String>,
    /// List of names for the new entities to be spawned, has to be the same
    /// length as `entity_prefabs`
    pub entity_names: Vec<String>,
}
impl Payload for SpawnEntitiesRequest {
    fn r#type(&self) -> MessageType {
        MessageType::SpawnEntitiesRequest
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct SpawnEntitiesResponse {
    /// Names of entities that were spawned as the result of the request,
    /// order from the request is preserved
    pub entity_names: Vec<String>,
    pub error: String,
}
impl Payload for SpawnEntitiesResponse {
    fn r#type(&self) -> MessageType {
        MessageType::SpawnEntitiesResponse
    }
}

/// Requests the server to export a snapshot.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ExportSnapshotRequest {
    /// Name for the snapshot file
    pub name: String,
    /// Whether to save created snapshot to disk locally on the server.
    pub save_to_disk: bool,
    /// Whether the snapshot should be send back.
    pub send_back: bool,
}
impl Payload for ExportSnapshotRequest {
    fn r#type(&self) -> MessageType {
        MessageType::ExportSnapshotRequest
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ExportSnapshotResponse {
    pub error: String,
    pub snapshot: Vec<u8>,
}
impl Payload for ExportSnapshotResponse {
    fn r#type(&self) -> MessageType {
        MessageType::ExportSnapshotResponse
    }
}

/// Requests the server to list all local (available on the
/// server) scenarios.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ListLocalScenariosRequest {}
impl Payload for ListLocalScenariosRequest {
    fn r#type(&self) -> MessageType {
        MessageType::ListLocalScenariosRequest
    }
}

/// Response to `ListLocalScenariosRequest`.
///
/// `scenarios` contains a list of scenarios available locally
/// on the server that can be loaded.
///
/// `error` can contain:
/// - `NoScenariosFound`
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ListLocalScenariosResponse {
    pub scenarios: Vec<String>,
    pub error: String,
}
impl Payload for ListLocalScenariosResponse {
    fn r#type(&self) -> MessageType {
        MessageType::ListLocalScenariosResponse
    }
}

/// Requests the server to load a local (available on the
/// server) scenario using the provided scenario name.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct LoadLocalScenarioRequest {
    pub scenario: String,
}
impl Payload for LoadLocalScenarioRequest {
    fn r#type(&self) -> MessageType {
        MessageType::LoadLocalScenarioRequest
    }
}

/// Response to `LoadLocalScenarioRequest`.
///
/// `error` can contain:
/// - `ScenarioNotFound`
/// - `FailedCreatingSimInstance`
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct LoadLocalScenarioResponse {
    pub error: String,
}
impl Payload for LoadLocalScenarioResponse {
    fn r#type(&self) -> MessageType {
        MessageType::LoadLocalScenarioResponse
    }
}

/// Requests the server to load a scenario included in the message.
/// Scenario data here is user files as collections of bytes.
///
/// `scenario_manifest` is the manifest file of the scenario.
///
/// `modules` contains a list of modules, each _module_ being
/// itself a collection of files. Files for each module are
/// laid out "flat", regardless of how they may have originally
/// been organized into multiple directories, etc.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct LoadRemoteScenarioRequest {
    pub scenario_manifest: Vec<u8>,
    pub modules: Vec<Vec<u8>>,
}
impl Payload for LoadRemoteScenarioRequest {
    fn r#type(&self) -> MessageType {
        MessageType::LoadRemoteScenarioRequest
    }
}

/// Response to `LoadRemoteScenarioRequest`.
///
/// `error` can contain:
/// - `FailedCreatingSimInstance`
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct LoadRemoteScenarioResponse {
    pub error: String,
}
impl Payload for LoadRemoteScenarioResponse {
    fn r#type(&self) -> MessageType {
        MessageType::LoadRemoteScenarioResponse
    }
}

// #[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
// pub struct InitializeNodeRequest {
//     pub model: String,
// }

// #[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize, Serialize)]
// pub struct StringAddress {
//     pub entity: String,
//     pub component: String,
//     pub var_type: u8,
//     pub var_name: String,
// }
//
// impl From<Address> for StringAddress {
//     fn from(a: Address) -> Self {
//         Self {
//             entity: a.entity.to_string(),
//             component: a.component.to_string(),
//             var_type: a.var_type.to_string(),
//             var_name: a.var_id.to_string(),
//         }
//     }
// }
//
// impl From<&Address> for StringAddress {
//     fn from(a: &Address) -> Self {
//         Self {
//             entity: a.entity.to_string(),
//             component: a.component.to_string(),
//             var_type: a.var_type.to_string(),
//             var_name: a.var_id.to_string(),
//         }
//     }
// }
//
// impl From<StringAddress> for Address {
//     fn from(a: StringAddress) -> Self {
//         Self {
//             entity: bigworlds::EntityName::from(&a.entity).unwrap(),
//             component: bigworlds::CompName::from(&a.component).unwrap(),
//             var_type: bigworlds::VarType::from_str_unchecked(&a.var_type),
//             var_id: bigworlds::VarName::from(&a.var_name).unwrap(),
//         }
//     }
// }
