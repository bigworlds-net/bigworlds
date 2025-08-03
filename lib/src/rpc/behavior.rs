use crate::{EntityName, EventName, QueryProduct};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Request {
    Event(EventName),
    Subscription(String, QueryProduct),
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Response {
    Empty,
}
