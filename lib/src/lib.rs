#![allow(unused)]

// NOTE: extern crate syntax is obsolete, but still provides global macro
// imports.
#[macro_use]
extern crate serde;
#[macro_use]
extern crate log;

pub mod address;
pub mod entity;
pub mod model;
pub mod var;

pub mod behavior;
pub mod net;
pub mod query;
pub mod rpc;
pub mod service;
pub mod sim;

#[cfg(feature = "machine")]
pub mod machine;

pub mod client;
pub mod leader;
pub mod node;
pub mod server;
pub mod worker;

mod string;
pub mod time;
pub mod util;

mod error;
mod executor;
mod snapshot;

pub use address::Address;
pub use error::{Error, Result};
pub use executor::{Executor, ExecutorMulti, LocalExec, RemoteExec, Signal};
pub use model::Model;
pub use query::{Query, QueryProduct};
pub use server::Config as ServerConfig;
pub use sim::SimHandle;
pub use snapshot::Snapshot;
pub use var::{Var, VarType};

const MODEL_MANIFEST_FILE: &str = "model.toml";

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
pub type EntityName = String;
/// Entity prefab string identifier.
pub type PrefabName = String;
/// Component string identifier.
pub type CompName = String;
/// Variable string identifier.
pub type VarName = String;
/// Event string identifier.
pub type EventName = String;

/// Entity unique integer identifier.
pub type EntityId = u32;
