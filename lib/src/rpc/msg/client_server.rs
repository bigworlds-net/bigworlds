use std::collections::HashMap;
use std::net::SocketAddr;

use fnv::FnvHashMap;

use crate::net::{Encoding, Transport};
use crate::{Address, CompName, EntityId, Float, Int, Var, VarName, VarType};

use super::Message;

/// Requests a simple `PingResponse` message. Can be used to check
/// the connection to the server.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct PingRequest {
    pub bytes: Vec<u8>,
}

/// Response to `PingRequest` message.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct PingResponse {
    pub bytes: Vec<u8>,
}

/// Requests a few variables related to the current status of
/// the server.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct StatusRequest {
    pub format: String,
}

impl Into<Message> for StatusRequest {
    fn into(self) -> Message {
        Message::StatusRequest(self)
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
    pub uptime: u64,
    pub current_tick: usize,

    // pub model: Model,

    // Id of the worker this server is attached to.
    pub worker: String,
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
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RegisterClientRequest {
    pub name: String,
    pub is_blocking: bool,
    pub auth_pair: Option<(String, String)>,
    pub encodings: Vec<Encoding>,
    pub transports: Vec<Transport>,
}

/// Response to a `RegisterClientRequest` message.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct RegisterClientResponse {
    pub client_id: String,
    pub encoding: Encoding,
    pub transport: Transport,
    pub redirect_to: Option<SocketAddr>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct QueryRequest {
    pub query: crate::query::Query,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct NativeQueryRequest {
    pub query: crate::query::Query,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct NativeQueryResponse {
    pub query_product: crate::query::QueryProduct,
    pub error: Option<String>,
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct DataTransferResponse {
    pub data: TransferResponseData,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ScheduledDataTransferRequest {
    pub event_triggers: Vec<String>,
    pub transfer_type: String,
    pub selection: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct VarSimDataPackOrdered {
    pub vars: Vec<crate::Var>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct VarSimDataPack {
    pub vars: FnvHashMap<(crate::EntityName, crate::CompName, crate::VarName), crate::Var>,
}

/// Structure holding all data organized based on data types.
///
/// Each data type is represented by a set of key-value pairs, where
/// keys are addresses represented with strings.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct TypedSimDataPack {
    pub strings: HashMap<Address, String>,
    pub ints: HashMap<Address, crate::Int>,
    pub floats: HashMap<Address, crate::Float>,
    pub bools: HashMap<Address, bool>,
    pub string_lists: HashMap<Address, Vec<String>>,
    pub int_lists: HashMap<Address, Vec<crate::Int>>,
    pub float_lists: HashMap<Address, Vec<crate::Float>>,
    pub bool_lists: HashMap<Address, Vec<bool>>,
    pub string_grids: HashMap<Address, Vec<Vec<String>>>,
    pub int_grids: HashMap<Address, Vec<Vec<crate::Int>>>,
    pub float_grids: HashMap<Address, Vec<Vec<crate::Float>>>,
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
    pub fn from_query_product(qp: crate::query::QueryProduct) -> Self {
        let mut data = Self::empty();
        match qp {
            crate::query::QueryProduct::AddressedTyped(atm) => {
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct TypedDataTransferResponse {
    pub data: TypedSimDataPack,
    pub error: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct DataPullRequest {
    pub data: PullRequestData,
}

/// Response to `DataPullRequest`.
///
/// `error` contains the report of any errors that might have occurred.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct DataPullResponse {
    pub error: String,
}

/// Request the server to pull provided data into the main simulation
/// database.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct TypedDataPullRequest {
    pub data: TypedSimDataPack,
}

/// Response to `DataPullRequest`.
///
/// `error` contains the report of any errors that might have occurred.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct TypedDataPullResponse {
    pub error: String,
}

/// Requests an advancement of the simulation by one or more steps.
///
/// In a situation with multiple blocking clients, this request acts
/// as a "thumbs up" signal from the client sending it. Until all
/// blocking clients have sent the signal that they are _ready_,
/// processing cannot continue.
///
/// This request is only valid for clients that are _blocking_.
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
pub struct AdvanceRequest {
    /// Number of steps to advance the simulation by
    pub step_count: u32,
    /// Require response to be sent only once once the request was fulfilled
    pub wait: bool,
}

/// Response to `TurnAdvanceRequest`.
///
/// `error` contains report of errors if any were encountered.
/// Possible errors include:
/// - `ClientIsNotBlocking`
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct AdvanceResponse {
    pub error: String,
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

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct SpawnEntitiesResponse {
    /// Names of entities that were spawned as the result of the request,
    /// order from the request is preserved
    pub entity_names: Vec<String>,
    pub error: String,
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

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ExportSnapshotResponse {
    pub error: String,
    pub snapshot: Vec<u8>,
}

/// Requests the server to list all scenarios for the currently loaded model.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ListScenariosRequest {}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ListScenariosResponse {
    pub scenarios: Vec<String>,
    pub error: String,
}

/// Requests the server to initialize new simulation run using the provided
/// scenario name.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct LoadScenarioRequest {
    pub scenario: String,
}

impl Into<Message> for LoadScenarioRequest {
    fn into(self) -> Message {
        Message::LoadScenarioRequest(self)
    }
}

/// `error` can contain:
/// - `ScenarioNotFound`
/// - `FailedCreatingSimInstance`
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct LoadScenarioResponse {
    pub error: String,
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

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct UploadProjectRequest {
    pub archive: Vec<u8>,
}

impl Into<Message> for UploadProjectRequest {
    fn into(self) -> Message {
        Message::UploadProjectArchiveRequest(self)
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct UploadProjectResponse {
    pub error: String,
}

impl Into<Message> for UploadProjectResponse {
    fn into(self) -> Message {
        Message::UploadProjectArchiveResponse(self)
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
