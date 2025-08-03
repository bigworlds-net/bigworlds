use std::borrow::Borrow;
use std::sync::Arc;

use tokio::sync::Mutex;
use uuid::Uuid;

use crate::executor::Executor;
use crate::query::{process_query, Query, QueryProduct};
use crate::string;

use crate::rpc;
use crate::rpc::msg::*;
use crate::server::{ClientId, Server};
use crate::{Error, Result};

/// Attempts to process incoming advance request.
///
/// # Process
///
/// This function will return a response back to calling client only after
/// the cluster reaches the target clock value.
pub async fn handle_advance_request(
    server: Arc<Mutex<Server>>,
    req: AdvanceRequest,
    client_id: ClientId,
) -> Result<Message> {
    if !server
        .lock()
        .await
        .clients
        .get(&client_id)
        .unwrap()
        .is_blocking
    {
        return Err(Error::Other(
            "Non-blocking client sent advance request".to_string(),
        ));
    }

    // Make sure the step_count value is valid. Return immediately if not.
    if req.step_count < 1 {
        return Ok(Message::AdvanceResponse(AdvanceResponse {
            error: format!("invalid `step_count` value: {}", req.step_count),
        }));
    }

    // Access clock watch and determine target clock value.
    let mut clock_watch = server.lock().await.clock.1.clone();
    let current_clock = clock_watch.borrow().clone();
    let mut target_clock = current_clock + req.step_count as usize;

    // Set the calling client as not blocking until the target clock is
    // reached.
    // server
    //     .lock()
    //     .await
    //     .clients
    //     .get_mut(&client_id)
    //     .unwrap()
    //     .unblocked_until
    //     .0
    //     .send(Some(target_clock));
    server.lock().await.blocked.0.send_modify(|c| *c -= 1);

    trace!("waiting for clock to change..");

    while clock_watch.changed().await.is_ok() {
        let current_clock = clock_watch.borrow().clone();
        println!(
            "current clock changed to: {}, target clock: {}",
            current_clock, target_clock
        );
        if current_clock == target_clock {
            // Set the client as blocking again.
            // server
            //     .lock()
            //     .await
            //     .clients
            //     .get_mut(&client_id)
            //     .unwrap()
            //     .unblocked_until
            //     .0
            //     .send(None);
            server.lock().await.blocked.0.send_modify(|c| *c += 1);
            println!(
                "server blocked: added 1: {}",
                *server.lock().await.blocked.1.borrow()
            );

            return Ok(Message::AdvanceResponse(AdvanceResponse {
                error: "".to_string(),
            }));
        } else if current_clock > target_clock {
            return Err(Error::Other(format!(
                "clock went too far compared to target"
            )));
        } else {
            continue;
        }
    }

    debug!("clock changed");

    Ok(Message::AdvanceResponse(AdvanceResponse {
        error: "".to_string(),
    }))
}

async fn is_blocking(current: usize, target: usize) -> bool {
    current > target
}

// /// Attempts to process incoming turn advance request.
// ///
// /// # Process
// ///
// /// This function will return a response back to calling client only after
// /// the cluster reaches the target clock value.
// pub async fn handle_step_request2(
//     server: Arc<Mutex<Server>>,
//     req: AdvanceRequest,
//     client_id: ClientId,
// ) -> Result<Message> {
//     debug!("server handling turn advance request");

//     let mut client_furthest_step = 0;

//     // flag making note of any blocking clients
//     let mut no_blocking_clients = true;

//     // get current clock state from worker
//     let mut step_before_advance = match server
//         .lock()
//         .await
//         .worker
//         .as_ref()
//         .ok_or(Error::WorkerNotConnected("".to_string()))?
//         .execute(Signal::from(rpc::worker::Request::Clock))
//         .await?
//     {
//         rpc::worker::Response::Clock(clock) => clock,
//         r => {
//             println!("response: {:?}", r);
//             return Err(Error::Other("wrong response type".to_string()));
//         }
//     };

//     println!("step_before_advance: {}", step_before_advance);

//     trace!("step count before advance attempt: {}", step_before_advance);
//     let mut common_furthest_step = step_before_advance + 99999;

//     if let Some(_client) = server.lock().await.clients.get_mut(&client_id) {
//         trace!(
//             "[client_id: {}] current_step: {}, furthest_step: {:?}",
//             client_id,
//             step_before_advance,
//             _client.furthest_step,
//         );

//         if _client.furthest_step < step_before_advance {
//             _client.furthest_step = step_before_advance;
//         }
//         if _client.furthest_step - step_before_advance < req.step_count as usize {
//             _client.furthest_step = _client.furthest_step + req.step_count as usize;
//         }
//         client_furthest_step = _client.furthest_step;
//     }

//     for (id, _client) in &mut server.lock().await.clients {
//         if !_client.is_blocking {
//             trace!("omit non-blocking client..");
//             continue;
//         } else {
//             no_blocking_clients = false;
//         }
//         trace!(
//             "client_furthest_tick inside loop: {}",
//             _client.furthest_step
//         );
//         // if _client.furthest_step < current_step {
//         //     common_furthest_step = current_step;
//         //     break;
//         // }
//         if _client.furthest_step < common_furthest_step {
//             common_furthest_step = _client.furthest_step;
//         }
//     }

//     if no_blocking_clients {
//         if let Some(client) = server.lock().await.clients.get(&client_id) {
//             common_furthest_step = client.furthest_step;
//         }
//     } else {
//         server
//             .lock()
//             .await
//             .worker
//             .as_ref()
//             .ok_or(Error::WorkerNotConnected("".to_string()))?
//             .execute(rpc::worker::Request::SetBlocking(true))
//             .await?;
//     }

//     let mut clock_after_advance = step_before_advance;
//     trace!(
//         "common_furthest_step: {}, step_before_advance: {}",
//         common_furthest_step,
//         step_before_advance
//     );
//     if common_furthest_step > step_before_advance {
//         use rpc::worker::{Request, RequestLocal, Response};

//         debug!("sending step request to worker");

//         // TODO
//         // let resp = self
//         //     .worker
//         //     .server_exec
//         //     .execute(RequestLocal::Request(Request::Step {
//         //         target_step: common_furthest_step,
//         //     }))
//         //     .await??;
//         //
//         // if let Response::Empty = resp {
//         //     return Ok(Message::TurnAdvanceResponse(StepResponse {
//         //         error: "".to_string(),
//         //     }));
//         // }

//         // TODO
//         return Ok(Message::AdvanceResponse(AdvanceResponse {
//             error: "".to_string(),
//         }));

//         // // worker can't initiate step processing on it's own, it
//         // // has to signal to the leader
//         //
//         // // first let them know the worker is ready
//         // worker
//         //     .net
//         //     .sig_send_central(TaskId::new_v4(), engine_core::distr::Signal::WorkerReady)?;
//         //
//         // // request leader to step cluster forward
//         //
//         // // register a local task keeping track of the request
//         // // let task_id =
//         // //     worker.register_task(WorkerTask::RequestedLeaderToProcessStep(false))?;
//         // // self.tasks
//         // //     .insert(task_id, ServerTask::WaitForOrganStepAdvance(*client_id));
//         //
//         // // id is attached to the request so that when incoming signals are
//         // // read on the worker, it knows what the response is related to
//         // worker.net.sig_send_central(
//         //     Uuid::new_v4(),
//         //     engine_core::distr::Signal::WorkerStepAdvanceRequest(
//         //         (common_furthest_step - step_before_advance) as u32,
//         //     ),
//         // )?;
//         // // once leader receives this request, it will store that
//         // // particular worker's furthest step, similar to how this server
//         // // is doing for it's clients
//         // //
//         // // leader evaluates readiness of all other workers, and if all
//         // // are ready it will initiate a new step
//         // //
//         // // if other workers are not ready, leader response to this
//         // // message will be delayed
//         //
//         // return Ok(());
//     } else {
//         server
//             .lock()
//             .await
//             .worker
//             .as_ref()
//             .ok_or(Error::WorkerNotConnected("".to_string()))?
//             .execute(rpc::worker::Request::SetBlocking(true))
//             .await?;
//     }

//     let mut _server = server.lock().await;
//     let client = _server.clients.get_mut(&client_id).unwrap();

//     // clock wasn't moved
//     // if common_furthest_step == current_step {
//     if common_furthest_step == step_before_advance {
//         trace!("BlockedFully");
//         // client.scheduled_advance_response = Some(client.)
//         // immediate response requested
//         if !req.wait {
//             let resp = Message::AdvanceResponse(AdvanceResponse {
//                 error: "BlockedFully".to_string(),
//             });
//             // client.connection.send_obj(resp, None)?;
//         } else {
//             client.scheduled_advance_response = Some(client.furthest_step);
//         }
//     } else if common_furthest_step < client_furthest_step {
//         trace!("BlockedPartially");
//         if !req.wait {
//             let resp = Message::AdvanceResponse(AdvanceResponse {
//                 error: "BlockedPartially".to_string(),
//             });
//             // client.connection.send_obj(resp, None)?;
//         } else {
//             client.scheduled_advance_response = Some(client.furthest_step);
//         }
//         //        } else if common_furthest_tick == client_furthest_tick {
//     } else {
//         trace!("Didn't block");
//         let resp = Message::AdvanceResponse(AdvanceResponse {
//             error: String::new(),
//         });
//         // client.connection.send_obj(resp, None)?;
//     }

//     // // check the clients for scheduled step advance responses
//     // for (_, client) in &mut self.clients {
//     //     if &client.id == client_id {
//     //         continue;
//     //     }
//     //     if let Some(scheduled_step) = client.scheduled_advance_response {
//     //         warn!:
//     //             "[client: {}] scheduled_step: {}, current_step: {}",
//     //             client.id, scheduled_step, clock_after_advance
//     //         );
//     //         if scheduled_step == clock_after_advance {
//     //             let resp = TurnAdvanceResponse {
//     //                 error: String::new(),
//     //             };
//     //             client.connection.send_payload(resp, None)?;
//     //             client.scheduled_advance_response = None;
//     //         }
//     //     }
//     // }

//     unimplemented!()
// }
