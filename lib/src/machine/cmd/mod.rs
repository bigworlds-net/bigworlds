//! Command definitions for the machine DSL.
//!
//! Command struct serves as the basic building block for the in-memory logic
//! representation used by the runtime. Each command provides an implementation
//! for it's interpretation (converting a command prototype into target struct)
//! and for it's execution (performing work, usually work on some piece of
//! data).
//!
//! Individual machine behaviors, as seen declared within a model, exist
//! as collections of command structs. During logic execution, these
//! collections are iterated on, executing the commands one by one.

#![allow(unused)]

pub mod print;
pub mod register;
// pub mod equal;
// pub mod attach;
// pub mod detach;
pub mod eval;
pub mod flow;
pub mod goto;
pub mod invoke;
// pub mod jump;
pub mod spawn;
// pub mod range;
pub mod set;
// pub mod sim;
pub mod query;

// #[cfg(feature = "machine_dynlib")]
// pub mod lib;

pub use spawn::Spawn;

use std::collections::{BTreeMap, HashMap};
use std::env::args;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use arrayvec::ArrayVec;
use fnv::FnvHashMap;
use smallvec::SmallVec;

#[cfg(feature = "machine_dynlib")]
use libloading::Library;

use crate::address::{Address, ShortLocalAddress};
use crate::{CompName, EntityId, EntityName, Var, VarType};

use crate::{model, util};

use crate::entity::{Entity, Storage};
use crate::model::Model;

// #[cfg(feature = "machine_dynlib")]
// use crate::machine::cmd::lib::LibCall;

use crate::executor::{Executor, LocalExec};
use crate::machine::cmd::CommandResult::JumpToLine;
use crate::machine::error::{Error, ErrorKind, Result};
use crate::machine::{CommandPrototype, CommandResultVec, ExecutionContext, LocationInfo};

use super::{Logic, Machine};

/// Used for controlling the flow of execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandResult {
    /// Continue execution.
    Continue,
    /// Break execution.
    Break,
    /// Jump to line.
    JumpToLine(usize),
    /// Jump to tag.
    JumpToTag(String),
    /// Signalize that an error has occurred during execution of the command.
    Err(Error),
}

impl CommandResult {
    pub fn from_str(s: &str) -> Option<CommandResult> {
        if s.starts_with("jump.") {
            let c = &s[5..];
            return Some(CommandResult::JumpToLine(c.parse::<usize>().unwrap()));
        }
        //else if s.starts_with("goto.") {
        //let c = &s[5..];
        //return Some(CommandResult::GoToState(SmallString::from_str(c).unwrap()));
        //}
        else {
            match s {
                "ok" | "Ok" | "OK" | "continue" => Some(CommandResult::Continue),
                "break" => Some(CommandResult::Break),
                _ => None,
            }
        }
    }
}

/// Defines all the local commands.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Command {
    NoOp,

    Print(print::Print),
    PrintFmt(print::PrintFmt),
    // Sim(sim::SimControl),
    Set(set::Set),
    Eval(eval::Eval),
    // EvalReg(EvalReg),
    Query(query::Query),

    // Equal(Equal),
    // BiggerThan(BiggerThan),
    // #[cfg(feature = "machine_dynlib")]
    // LibCall(lib::LibCall),

    // Attach(attach::Attach),
    // Detach(detach::Detach),
    Goto(goto::Goto),
    // Jump(jump::Jump),
    Invoke(invoke::Invoke),
    Spawn(spawn::Spawn),

    // Registrations.
    // RegisterEvent(register::RegisterEvent),
    RegisterEntityPrefab(register::RegisterEntityPrefab),
    RegisterComponent(register::RegisterComponent),
    RegisterTrigger(register::RegisterTrigger),
    RegisterVar(register::RegisterVar),
    // Extend(register::Extend),

    // Register blocks.
    Machine(flow::machine::MachineBlock),
    State(flow::state::State),
    Component(flow::component::ComponentBlock),

    // Flow control
    If(flow::ifelse::If),
    Else(flow::ifelse::Else),
    End(flow::end::End),
    Call(flow::call::Call),
    ForIn(flow::forin::ForIn),
    Loop(flow::_loop::Loop),
    Break(flow::_loop::Break),
    Procedure(flow::procedure::Procedure),
    //
    // Range(range::Range),
}

impl Command {
    /// Creates new command struct from a prototype.
    pub fn from_prototype(
        proto: &CommandPrototype,
        location: &LocationInfo,
        commands: &Vec<CommandPrototype>,
    ) -> Result<Command> {
        let cmd_name = match &proto.name {
            Some(c) => c,
            None => return Err(Error::new(location.clone(), ErrorKind::NoCommandPresent)),
        };
        let args = match &proto.arguments {
            Some(a) => a.clone(),
            None => Vec::new(),
        };

        trace!(
            "command from prototype: args: {:?}, location: {:?}",
            args,
            location
        );

        match cmd_name.as_str() {
            "print" => Ok(Command::PrintFmt(print::PrintFmt::from_args(
                args, location,
            )?)),
            "set" => Ok(set::Set::new(args, location)?),
            // // "set" => Ok(get::Get::new(args, location)?),
            "spawn" => Ok(Command::Spawn(spawn::Spawn::new(args, location)?)),
            "invoke" => Ok(Command::Invoke(invoke::Invoke::new(args)?)),
            "query" => query::Query::new(args, location, &commands),
            // "sim" => Ok(sim::SimControl::new(args)?),
            //
            // "extend" => Ok(Command::Extend(register::Extend::new(args, location)?)),
            //
            // // register one-liners
            // "event" => Ok(Command::RegisterEvent(register::RegisterEvent::new(
            //     args, location,
            // )?)),
            "entity" | "prefab" => Ok(Command::RegisterEntityPrefab(
                register::RegisterEntityPrefab::new(args, location)?,
            )),
            "trigger" | "triggered_by" => Ok(Command::RegisterTrigger(
                register::RegisterTrigger::new(args, location)?,
            )),
            "var" => Ok(Command::RegisterVar(register::RegisterVar::new(
                args, location,
            )?)),

            "machine" => Ok(flow::machine::MachineBlock::new(args, location, &commands)?),

            "component" | "comp" => {
                Ok(register::RegisterComponent::new(args, location, &commands)?)
            }
            //
            // // register blocks
            // // "component" | "comp" => Ok(flow::component::ComponentBlock::new(
            // //     args, location, &commands,
            // // )?),
            "state" => Ok(flow::state::State::new(args, location, &commands)?),
            "goto" => Ok(Command::Goto(goto::Goto::new(args)?)),
            //
            // // flow control
            // "jump" => Ok(Command::Jump(jump::Jump::new(args)?)),
            "if" => Ok(Command::If(flow::ifelse::If::new(
                args, location, &commands,
            )?)),
            "else" => Ok(Command::Else(flow::ifelse::Else::new(args)?)),
            "proc" | "procedure" => Ok(Command::Procedure(flow::procedure::Procedure::new(
                args, location, &commands,
            )?)),
            "call" => Ok(Command::Call(flow::call::Call::new(
                args, location, &commands,
            )?)),
            "end" => Ok(Command::End(flow::end::End::new(args)?)),
            "for" => Ok(Command::ForIn(flow::forin::ForIn::new(
                args, location, commands,
            )?)),
            "loop" | "while" => Ok(Command::Loop(flow::_loop::Loop::new(
                args, location, commands,
            )?)),
            "break" => Ok(Command::Break(flow::_loop::Break {})),
            //
            // "range" => Ok(Command::Range(range::Range::new(args)?)),
            //
            "eval" => Ok(Command::Eval(eval::Eval::new(args)?)),
            //
            // #[cfg(feature = "machine_dynlib")]
            // "lib_call" => Ok(LibCall::new(args)?),
            _ => Err(Error::new(
                location.clone(),
                ErrorKind::UnknownCommand(cmd_name.to_string()),
            )),
        }
    }

    pub async fn execute(
        &self,
        machine: &mut Machine,
        mut logic: &mut Logic,
        call_stack: &mut super::CallStackVec,
        registry: &mut super::Registry,
        // comp_name: &CompName,
        // ent_id: &EntityId,
        location: &LocationInfo,
    ) -> CommandResult {
        trace!("executing command: {:?}", self);
        match self {
            // Command::Sim(cmd) => {
            //     out_res.push(cmd.execute_loc(ent_storage, comp_state, comp_name, location))
            // }
            // Command::Print(cmd) => out_res.push(cmd.execute_loc(ent_storage)),
            Command::PrintFmt(cmd) => cmd.execute(machine).await,
            Command::Set(cmd) => cmd.execute(machine).await,
            // Command::SetIntIntAddr(cmd) => {
            //     out_res.push(cmd.execute_loc(ent_storage, comp_name, location))
            // }
            //
            Command::Eval(cmd) => cmd.execute(machine, registry, location).await,
            // // Command::EvalReg(cmd) => out_res.push(cmd.execute_loc(registry)),
            //
            // //Command::Eval(cmd) => out_res.push(cmd.execute_loc(ent_storage)),
            // //Command::Equal(cmd) => out_res.push(cmd.execute_loc(ent_storage)),
            // //Command::BiggerThan(cmd) => out_res.push(cmd.execute_loc(ent_storage)),
            //
            // #[cfg(feature = "machine_dynlib")]
            // Command::LibCall(cmd) => out_res.push(cmd.execute_loc(libs, ent_id, ent_storage)),
            // //Command::Attach(cmd) => out_res.push(cmd.execute_loc(ent, sim_model)),
            // //Command::Detach(cmd) => out_res.push(cmd.execute_loc(ent, sim_model)),
            Command::Goto(cmd) => cmd.execute(&mut machine.state),
            // Command::Jump(cmd) => cmd.execute_loc(),
            //
            // Command::Get(cmd) => cmd.execute().await,
            Command::RegisterEntityPrefab(cmd) => cmd.execute(machine).await,
            Command::RegisterComponent(cmd) => cmd.execute(machine).await,
            Command::RegisterVar(cmd) => cmd.execute(machine, call_stack).await,
            // Command::RegisterTrigger(cmd) => out_res.extend(cmd.execute_loc(call_stack)),
            // Command::RegisterEvent(cmd) => out_res.extend(cmd.execute_loc()),
            //
            Command::Invoke(cmd) => cmd.execute(machine.clone()).await,
            Command::Spawn(cmd) => cmd.execute(machine).await,
            Command::Call(cmd) => cmd.execute(machine, logic, call_stack, location).await,
            //
            // Command::Jump(cmd) => out_res.push(cmd.execute_loc()),
            Command::If(cmd) => cmd.execute(machine, call_stack, location).await,
            // Command::Else(cmd) => out_res.push(cmd.execute_loc(call_stack, ent_storage, location)),
            // Command::ForIn(cmd) => out_res.push(cmd.execute_loc(
            //     call_stack,
            //     registry,
            //     comp_name,
            //     ent_storage,
            //     location,
            // )),
            Command::Loop(cmd) => cmd.execute_loc(call_stack),
            // Command::Break(cmd) => out_res.push(cmd.execute_loc(call_stack, ent_storage, location)),
            //
            Command::End(cmd) => cmd.execute(machine, call_stack, location).await,
            Command::Procedure(cmd) => cmd.execute(call_stack, location),

            Command::State(cmd) => cmd.execute(machine, &mut logic, call_stack, location),
            Command::Component(cmd) => cmd.execute(machine, call_stack).await,
            //
            // Command::Extend(cmd) => out_res.push(cmd.execute_loc()),
            // // Command::Register(cmd) => out_res.extend(cmd.execute_loc(call_stack)),
            // Command::Range(cmd) => out_res.push(cmd.execute_loc(ent_storage, comp_name, location)),
            _ => {
                warn!("unable to execute cmd: {:?}", self);
                CommandResult::Continue
            }
        }
    }
    pub fn run_with_model_context(&self, sim_model: &mut Model) -> CommandResult {
        match self {
            // Command::Register(cmd) => cmd.execute(sim_model),
            // Command::Print(cmd) => cmd.execute_loc(),
            _ => CommandResult::Continue,
        }
    }
}
