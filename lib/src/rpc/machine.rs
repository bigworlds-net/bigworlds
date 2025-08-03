use crate::{machine::cmd::Command, EventName};

use super::behavior;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Request {
    Execute(Command),
    ExecuteBatch(Vec<Command>),
    Step,

    // Event(EventName),
    Behavior(super::behavior::Request),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Response {
    Empty,
}
