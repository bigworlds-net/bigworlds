use chrono::Duration;
use serde::{Deserialize, Serialize};

use crate::executor::{LocalExec, Signal};
use crate::net::CompositeAddress;
use crate::server::ServerId;
use crate::worker::WorkerId;
use crate::Result;

use super::{msg::Message, worker};

#[derive(Clone)]
pub enum RequestLocal {
    ConnectToWorker(
        LocalExec<Signal<worker::RequestLocal>, Result<Signal<worker::Response>>>,
        LocalExec<Signal<RequestLocal>, Result<Signal<Response>>>,
    ),
    IntroduceWorker(LocalExec<Signal<worker::RequestLocal>, Result<Signal<worker::Response>>>),
    Request(Request),
}

impl From<Request> for RequestLocal {
    fn from(value: Request) -> Self {
        Self::Request(value)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, strum::Display)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Request {
    ConnectToWorker { address: CompositeAddress },
    Status,
    Message(Message),

    Redirect,
    ClockChangedTo(usize),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Response {
    Empty,
    Status { uptime_secs: u64, clients: u32 },
    ConnectToWorker { server_id: ServerId },
    IntroduceWorker(ServerId),
    Message(Message),

    Redirect,
}
