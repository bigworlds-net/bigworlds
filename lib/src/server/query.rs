use std::convert::TryInto;
use uuid::Uuid;

// use bigworlds::msg::{
//     DataTransferResponse, Message, NativeQueryRequest, NativeQueryResponse,
// QueryRequest,     TransferResponseData,
// };
use crate::query::{process_query, Description, Filter, Layout, Map, Query, QueryProduct, Trigger};
use crate::server::{ClientId, Server};
use crate::{string, Error, Result};

impl Server {
    // pub fn handle_query_request_compat(
    //     &mut self,
    //     msg: compat::Message,
    //     client_id: &ClientId,
    // ) -> Result<()> {
    //     info!("handling compat query request");
    //     let mut client = self.clients.get_mut(client_id).unwrap();
    //     let qr: compat::QueryRequest = msg.unpack_payload(client.connection.encoding())?;
    //
    //     match &mut self.sim {
    //         SimConnection::Local(sim) => {
    //             let query = qr.query;
    //
    //             if query.trigger.r#type == compat::TriggerType::Event {
    //                 if let Some(event_name) = query.trigger.args.first() {
    //                     let task_id = Uuid::new_v4();
    //                     client.push_event_triggered_query(
    //                         string::new_truncate(event_name),
    //                         task_id,
    //                         query.try_into()?,
    //                     )?;
    //                 }
    //             } else if query.trigger.r#type == compat::TriggerType::Mutation {
    //                 unimplemented!()
    //             } else {
    //                 info!("query trigger: {:?}", query.trigger);
    //                 // let insta = std::time::Instant::now();
    //                 let product =
    //                     process_query(&query.try_into()?, &sim.entities, &sim.entity_idx)?;
    //                 // println!(
    //                 //     "processing query took: {} ms",
    //                 //     Instant::now().duration_since(insta).as_millis()
    //                 // );
    //                 // let mut data_pack = SimDataPack::empty();
    //                 println!("product: {:?}", product);
    //                 if let QueryProduct::AddressedVar(map) = product {
    //                     let resp = compat::Message::from_payload(
    //                         compat::DataTransferResponse {
    //                             data: compat::TransferResponseData::AddressedVar(map),
    //                         },
    //                         client.connection.encoding(),
    //                     )?;
    //                     client.connection.send_obj(resp, None)?;
    //                 }
    //                 // println!("msg taskid: {}", msg.task_id);
    //             }
    //         }
    //         SimConnection::Leader(org) => {
    //             // TODO real query
    //             let query = Query {
    //                 trigger: Trigger::Event(string::new_truncate("step")),
    //                 description: Description::Addressed,
    //                 layout: Layout::Typed,
    //                 filters: vec![Filter::AllComponents(vec![string::new_truncate(
    //                     "transform",
    //                 )])],
    //                 mappings: vec![Map::All],
    //             };
    //
    //             let task_id = org.register_task(LeaderTask::WaitForQueryResponses {
    //                 remaining: org.net.workers.len() as u32,
    //                 products: vec![],
    //             })?;
    //             self.tasks
    //                 .insert(task_id, ServerTask::WaitForOrganQueryResponse(*client_id));
    //             org.net
    //                 .broadcast_sig(task_id, Signal::QueryRequest(query))?;
    //         }
    //
    //         SimConnection::Worker(worker) => {
    //             // // check if query wants local entities only
    //             // if query.filters.contains(&bigworlds::query::Filter::Node(0))
    //             // {}
    //         }
    //     }
    //
    //     Ok(())
    // }

    // pub fn handle_native_query_request(
    //     &mut self,
    //     msg: Message,
    //     client_id: &ClientId,
    // ) -> Result<()> {
    //     let mut client = self.clients.get_mut(client_id).unwrap();
    //     let qr: QueryRequest = msg.unpack_payload(client.connection.encoding())?;
    //
    //     match &mut self.sim {
    //         SimConnection::Local(sim) => {
    //             if let Trigger::Event(event_name) = &qr.query.trigger {
    //                 client.push_event_triggered_query(event_name.clone(),
    // msg.task_id, qr.query)?;             } else if let
    // Trigger::Mutation(address) = qr.query.trigger {
    // unimplemented!()             } else {
    //                 // println!("query: {:?}", qr.query);
    //
    //                 let product = process_query(&qr.query, &sim.entities,
    // &sim.entity_idx)?;                 client.connection.send_payload(
    //                     QueryResponse {
    //                         query_product: product,
    //                         error: None,
    //                     },
    //                     None,
    //                 )?;
    //             }
    //         }
    //         SimConnection::Leader(ref mut org) => {
    //             org.net.broadcast_sig(0, Signal::DataRequestAll);
    //             // leader.net.
    //             // TODO
    //         }
    //         SimConnection::Worker(worker) => {
    //             if let Some(node) = &worker.sim_node {
    //                 let product = process_query(&qr.query, &node.entities,
    // &node.entities_idx)?;                 client.connection.send_payload(
    //                     NativeQueryResponse {
    //                         query_product: product,
    //                         error: None,
    //                     },
    //                     None,
    //                 )?;
    //             }
    //         }
    //     }
    //     Ok(())
    // }
}
