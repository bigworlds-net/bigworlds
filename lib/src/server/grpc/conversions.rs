use crate::{rpc::msg::Message, server::grpc::service::protoworlds::SubscriptionId};

use super::service::protoworlds;

impl Into<Message> for protoworlds::SubscribeRequest {
    fn into(self) -> Message {
        Message::SubscribeRequest(vec![], crate::Query::default())
    }
}

impl From<Message> for protoworlds::SubscribeResponse {
    fn from(msg: Message) -> Self {
        match msg {
            Message::SubscribeResponse(id) => Self {
                message: Some(protoworlds::subscribe_response::Message::SubscriptionId(
                    SubscriptionId { id: id.to_string() },
                )),
            },
            Message::QueryResponse(product) => Self {
                message: Some(protoworlds::subscribe_response::Message::QueryResult(
                    protoworlds::QueryResult {
                        data: "".to_owned(),
                    },
                )),
            },
            _ => unimplemented!(),
        }
    }
}

impl Into<Message> for protoworlds::RegisterClientRequest {
    fn into(self) -> Message {
        Message::RegisterClientRequest(crate::rpc::msg::RegisterClientRequest {
            name: self.name,
            is_blocking: self.is_blocking,
            auth_pair: None,
            encodings: vec![],
            transports: vec![],
        })
    }
}

impl From<Message> for protoworlds::RegisterClientResponse {
    fn from(msg: Message) -> Self {
        Self {
            client_id: String::new(),
            encoding: 0,
            transport: 0,
            redirect_to: String::new(),
        }
    }
}
