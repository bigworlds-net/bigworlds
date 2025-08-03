use std::str::FromStr;

use fnv::FnvHashMap;

use crate::rpc::msg::Message;
use crate::rpc::msg::{DataPullRequest, DataPullResponse, PullRequestData, TypedDataPullRequest};
use crate::server::{ClientId, Server};
use crate::util_net::encode;
use crate::{Address, EntityId, Error, Result, Var};

impl Server {
    // pub fn handle_json_pull_request(&mut self, msg: Message, client_id:
    // &ClientId) -> Result<()> {     let mut client =
    // self.clients.get_mut(client_id).unwrap();     let req: JsonPullRequest =
    // match msg.unpack_payload(client.connection.encoding()) {         Ok(r) =>
    // r,         Err(e) => panic!("failed unpacking addressed_pull_request:
    // {}", e.to_string()),     };
    //     println!("json pull: {:?}", req);
    //
    //     if let SimConnection::Local(sim) = &mut self.sim {
    //         for (address, var) in req.data {
    //             if let Ok(v) = sim.get_var_mut(&address) {
    //                 *v = var.into();
    //             }
    //         }
    //     }
    //
    //     Ok(())
    // }

    // pub fn handle_data_pull_request(
    //     &mut self,
    //     dpr: DataPullRequest,
    //     client_id: &ClientId,
    // ) -> Result<()> {
    //     let mut client = self.clients.get_mut(client_id).unwrap();
    //
    //     let mut map = Vec::new();
    //     map.push((
    //         Address::from_str("2:transform:float:pos_x")?,
    //         Var::Float(111.),
    //     ));
    //     map.push((
    //         Address::from_str("2:transform:float:pos_y")?,
    //         Var::Float(222.),
    //     ));
    //     map.push((
    //         Address::from_str("2:transform:float:pos_z")?,
    //         Var::Float(333.),
    //     ));
    //
    //     // let mock = DataPullRequest {
    //     //     data: PullRequestData::AddressedVars(map),
    //     // };
    //     // let mock_msg = encode(mock, client.connection.encoding())?;
    //     // println!("mock: {:?}", mock_msg);
    //
    //     {
    //         let use_compression = self.config.use_compression.clone();
    //         // let sim_model = server.sim_model.clone();
    //         match &mut self.sim {
    //             SimCon::Local(sim) => {
    //                 // let dpr: DataPullRequest = msg.unpack_payload(client.connection.encoding())?;
    //                 // println!("dpr: {:?}", dpr);
    //                 match dpr.data {
    //                     PullRequestData::Typed(data) => {
    //                         // //TODO handle errors
    //                         // for (addr, var) in data.strings {
    //                         //     *sim.get_var_mut(&addr)?.as_string_mut()? = var;
    //                         // }
    //                         // for (addr, var) in data.ints {
    //                         //     *sim.get_var_mut(&addr)?.as_int_mut()? = var;
    //                         // }
    //                         // for (addr, var) in data.floats {
    //                         //     *sim.get_var_mut(&addr)?.as_float_mut()? = var;
    //                         // }
    //                         // for (addr, var) in data.bools {
    //                         //     *sim.get_var_mut(&addr)?.as_bool_mut()? = var;
    //                         // }
    //                         // for (addr, var) in data.string_lists {
    //                         //     *sim.get_var_mut(&addr)?.as_str_list_mut()? = var;
    //                         // }
    //                         // for (addr, var) in data.int_lists {
    //                         //     *sim.get_var_mut(&addr)?.as_int_list_mut()? = var;
    //                         // }
    //                         // for (addr, var) in data.float_lists {
    //                         //     *sim.get_var_mut(&addr)?.as_float_list_mut()? = var;
    //                         // }
    //                         // for (addr, var) in data.bool_lists {
    //                         //     *sim.get_var_mut(&addr)?.as_bool_list_mut()? = var;
    //                         // }
    //                         unimplemented!()
    //                     }
    //                     PullRequestData::NativeAddressedVars(data) => {
    //                         for ((ent, comp, var_name), v) in data.vars {
    //                             // let addr = Address::from_str(&k)?;
    //                             let ent_id = ent.parse::<EntityId>()?;
    //                             if let Some(entity) = sim.entities.get_mut(&ent_id) {
    //                                 if let Some(var) = entity.storage.map.get_mut(&(comp, var_name))
    //                                 {
    //                                     *var = v;
    //                                 }
    //                             }
    //                             // sim_instance *
    //                             // sim_instance.get_var_mut(&addr)? = v;
    //                         }
    //                     }
    //                     PullRequestData::VarOrdered(order_idx, data) => {
    //                         if let Some(order) = client.order_store.get(&order_idx) {
    //                             if data.vars.len() != order.len() {
    //                                 warn!("PullRequestData::VarOrdered: var list length doesn't match ({} vs {})", data.vars.len(), order.len());
    //                                 panic!();
    //                             }
    //                             for (n, addr) in order.iter().enumerate() {
    //                                 *sim.get_var_mut(addr)? = data.vars[n].clone();
    //                             }
    //                         }
    //                     }
    //                     PullRequestData::NativeAddressedVar((ent_id, comp_name, var_name), var) => {
    //                         if let Some(entity) = sim.entities.get_mut(&ent_id) {
    //                             if let Ok(v) = entity.storage.get_var_mut(&(comp_name, var_name)) {
    //                                 *v = var;
    //                                 // println!("pulled in {:?}", v);
    //                             }
    //                         }
    //                     }
    //                     PullRequestData::AddressedVars(data) => {
    //                         for (address, var) in data {
    //                             if let Ok(v) = sim.get_var_mut(&address) {
    //                                 *v = var;
    //                             }
    //                         }
    //                     }
    //                 }
    //             }
    //             SimCon::Leader(org) => {
    //                 // TODO
    //             }
    //             SimCon::Worker(worker) => {
    //                 //TODO
    //                 // let dpr: DataPullRequest = msg.unpack_payload(client.connection.encoding())?;
    //                 match dpr.data {
    //                     PullRequestData::NativeAddressedVars(data) => {
    //                         for ((ent, comp, var), v) in data.vars {
    //                             // let addr = Address::from_str(&k)?;
    //                             if let Some(sim_node) = worker.sim_node.as_mut() {
    //                                 if let Some(ent) =
    //                                     sim_node.entities.get_mut(&ent.parse().unwrap())
    //                                 {
    //                                     if let Some(var) = ent.storage.map.get_mut(&(comp, var)) {
    //                                         *var = v;
    //                                     }
    //                                 }
    //                             }
    //
    //                             // sim_instance *
    //                             // sim_instance.get_var_mut(&addr)? = v;
    //                         }
    //                     }
    //                     _ => unimplemented!(),
    //                 }
    //             }
    //         };
    //     }
    //     let resp = Message::DataPullResponse(DataPullResponse {
    //         error: String::new(),
    //     });
    //     Ok(())
    //     // client.connection.send_obj(resp, None).map_err(|e| e.into())
    // }

    //     pub fn handle_typed_data_pull_request(
    //         &mut self,
    //         tdpr: TypedDataPullRequest,
    //         client_id: &ClientId,
    //     ) -> Result<()> {
    //         let mut client = self.clients.get_mut(client_id).unwrap();
    //         let use_compression = self.config.use_compression.clone();
    //
    //         let data = tdpr.data;
    //
    //         // println!("typed data pull request: {:?}", data);
    //
    //         let mut sim_instance = match &mut self.sim {
    //             SimCon::Local(sim) => {
    //                 // //TODO handle errors
    //                 // for (addr, var) in data.strings {
    //                 //     *sim.get_var_mut(&addr)?.as_string_mut()? = var;
    //                 // }
    //                 // for (addr, var) in data.ints {
    //                 //     *sim.get_var_mut(&addr)?.as_int_mut()? = var;
    //                 // }
    //                 // for (addr, var) in data.floats {
    //                 //     *sim.get_var_mut(&addr)?.as_float_mut()? = var;
    //                 // }
    //                 // for (addr, var) in data.bools {
    //                 //     *sim.get_var_mut(&addr)?.as_bool_mut()? = var;
    //                 // }
    //                 // for (addr, var) in data.string_lists {
    //                 //     *sim.get_var_mut(&addr)?.as_str_list_mut()? = var;
    //                 // }
    //                 // for (addr, var) in data.int_lists {
    //                 //     *sim.get_var_mut(&addr)?.as_int_list_mut()? = var;
    //                 // }
    //                 // for (addr, var) in data.float_lists {
    //                 //     *sim.get_var_mut(&addr)?.as_float_list_mut()? = var;
    //                 // }
    //                 // for (addr, var) in data.bool_lists {
    //                 //     *sim.get_var_mut(&addr)?.as_bool_list_mut()? = var;
    //                 // }
    //                 unimplemented!();
    //
    //                 let resp = Message::DataPullResponse(DataPullResponse {
    //                     error: String::new(),
    //                 });
    //                 // send_message(message_from_payload(resp, false), stream, None);
    //                 // client.connection.send_obj(resp, None)?;
    //             }
    //             SimCon::Leader(org) => {
    //                 let mut data_vec = Vec::new();
    //                 for (fs, f) in data.floats {
    //                     data_vec.push((fs.into(), Var::Float(f)));
    //                 }
    //                 org.net
    //                     .broadcast_sig(TaskId::new_v4(), Signal::DataPullRequest(data_vec))?;
    //             }
    //             SimCon::Worker(worker) => unimplemented!(),
    //         };
    //
    //         Ok(())
    //     }
}
