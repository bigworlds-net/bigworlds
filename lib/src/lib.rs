#![allow(unused)]

// NOTE: extern crate syntax is obsolete, but still provides global macro
// imports.
#[macro_use]
extern crate serde;
#[macro_use]
extern crate log;

pub use error::{Error, Result};
pub use relay::Relay;
pub use server::Config as ServerConfig;
pub use sim::SimHandle;

pub use address::Address;
pub use executor::{Executor, LocalExec, RemoteExec, Signal};
pub use model::Model;
pub use query::{Query, QueryProduct};
pub use string::{LongString, ShortString, StringId};
pub use var::{Var, VarType};

pub mod address;
pub mod string;
pub mod time;
pub mod var;

pub mod client;
pub mod entity;
pub mod model;
pub mod net;
pub mod query;
pub mod rpc;
pub mod util;
pub mod util_net;

pub mod service;
pub mod sim;

pub mod leader;
pub mod node;
pub mod server;
pub mod worker;

pub mod behavior;
#[cfg(feature = "machine")]
pub mod machine;

mod error;
mod executor;
mod relay;

const MODEL_MANIFEST_FILE: &str = "model.toml";
const SNAPSHOTS_DIR_NAME: &str = "snapshots";

/// Floating point numer type used throughout the library.
#[cfg(feature = "big_nums")]
pub type Float = f64;
/// Floating point numer type used throughout the library.
#[cfg(not(feature = "big_nums"))]
pub type Float = f32;

/// Integer number type used throughout the library.
#[cfg(feature = "big_nums")]
pub type Int = i64;
/// Integer number type used throughout the library.
#[cfg(not(feature = "big_nums"))]
pub type Int = i32;

/// Entity string identifier.
pub type EntityName = StringId;
/// Entity prefab string identifier.
pub type PrefabName = StringId;
/// Component string identifier.
pub type CompName = StringId;
/// Variable string identifier.
pub type VarName = StringId;
/// Event string identifier.
pub type EventName = StringId;

/// Entity unique integer identifier.
pub type EntityId = u32;
