//! This module defines functionalist for dealing with executing command
//! collections within different contexts

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use fnv::FnvHashMap;
use itertools::Itertools;

use crate::entity::Storage;
use crate::machine::{ErrorKind, Result};
use crate::{rpc, CompName, EntityId, LocalExec};

#[cfg(feature = "machine_dynlib")]
use crate::machine::Libraries;

use super::cmd::{Command, CommandResult};
use super::error::Error;
use super::{CallStackVec, ExecutionContext, LocationInfo, Logic, Machine, Registry};

/// Executes given set of commands within global sim scope.
pub async fn execute(
    mut logic: Logic,
    machine: &mut Machine,
    // ent_id: &EntityId,
    // comp_uid: &CompName,
    // start: Option<usize>,
    // end: Option<usize>,
    #[cfg(feature = "machine_dynlib")] libs: &Libraries,
) -> Result<()> {
    // initialize a new call stack
    let mut call_stack = CallStackVec::new();
    let mut registry = Registry::new();

    let (start, end) = *logic.states.get(&machine.state).ok_or::<Error>(
        ErrorKind::Other(format!(
            "State `{}` not found in machine logic state collection",
            machine.state
        ))
        .into(),
    )?;

    let mut empty_locinfo = LocationInfo::empty();
    empty_locinfo.line = Some(0);

    // let mut cmd_n = match start {
    //     Some(s) => s,
    //     None => 0,
    // };
    let mut cmd_n = start;
    'outer: loop {
        // println!("cmd_n: {}", cmd_n);
        if cmd_n >= logic.commands.len() {
            break;
        }
        if call_stack.is_empty() && cmd_n >= end {
            break;
        }
        let current_cmd = logic.commands.get(cmd_n).unwrap().clone();
        // let location = model
        //     .get_component(comp_uid)
        //     .expect("can't get component model")
        //     .logic
        //     .cmd_location_map
        //     .get(cmd_n)
        //     .unwrap_or(&empty_locinfo)
        //     .clone();
        let location = logic
            .cmd_location_map
            .get(cmd_n)
            .unwrap_or(&empty_locinfo)
            .clone();

        // let entity = sim.entities.get_mut(sim.entities_idx.get(ent_uid).unwrap());
        // let mut comp_state: StringId = sim
        //     .execute(Signal::EntityCompStateRequest(
        //         ent_id.clone(),
        //         comp_uid.clone(),
        //     ))
        //     .await??
        //     .into_entity_comp_state_response()?;
        let result = current_cmd
            .execute(
                machine,
                &mut logic,
                &mut call_stack,
                &mut registry,
                // &model,
                &location,
                // #[cfg(feature = "machine_dynlib")]
                // libs,
            )
            .await;
        // println!("cmdresult: {:?}", result);
        match result {
            CommandResult::Continue => (),
            CommandResult::Break => break 'outer,
            CommandResult::JumpToLine(n) => {
                cmd_n = n;
                continue 'outer;
            }
            CommandResult::JumpToTag(_) => unimplemented!("jumping to tag not supported"),
            CommandResult::Err(e) => {
                //TODO implement configurable system for deciding whether to
                // break state, panic or just print when given error occurs
                error!("{}", e);
            }
        }
        cmd_n += 1;
    }
    Ok(())
}
