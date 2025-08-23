use std::sync::Arc;

use tokio::sync::Mutex;

use crate::rpc::msg::*;
use crate::server::{ClientId, State};
use crate::{Error, Result};

/// Attempts to process incoming advance request.
///
/// NOTE: this function will return a response back to calling client only
/// after the cluster reaches the target clock value.
pub async fn handle_advance_request(
    server: Arc<Mutex<State>>,
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
    let target_clock = current_clock + req.step_count as u64;

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
