use std::convert::TryInto;

use crate::query;
use crate::rpc::compat;
use crate::rpc::msg::Message;
use crate::string;

use super::{Description, Layout, Query, Trigger, TriggerType};

impl Into<Message> for compat::Message {
    fn into(self) -> Message {
        match self.type_ {
            compat::MessageType::PingRequest => Message::PingRequest(self.payload),
            compat::MessageType::PingResponse => Message::PingResponse(self.payload),
            // TODO all variants
            _ => unimplemented!(),
        }
    }
}

// impl TryInto<query::Query> for Query {
//     type Error = crate::Error;
//     fn try_into(self) -> Result<query::Query, Self::Error> {
//         Ok(crate::Query {
//             trigger: self.trigger.try_into()?,
//             description: query::Description::NativeDescribed,
//             layout: query::Layout::Var,
//             filters: vec![],
//             mappings: vec![],
//         })
//     }
// }

// impl TryInto<query::Trigger> for Trigger {
//     type Error = crate::Error;
//     fn try_into(self) -> Result<query::Trigger, Self::Error> {
//         match self.r#type {
//             TriggerType::Event => {
//                 let event = self
//                     .args
//                     .first()
//                     .ok_or(crate::Error::FailedConversion(format!("{:?}", self)))?;
//                 Ok(query::Trigger::Event(string::new_truncate(event)))
//             }
//             _ => unimplemented!(),
//         }
//     }
// }
