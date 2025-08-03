use smallvec::SmallVec;

use crate::entity::{Entity, Storage};
use crate::model::Model;

use super::super::super::{
    error::Error, CallInfo, CallStackVec, ForInCallInfo, IfElseCallInfo, IfElseMetaData,
    ProcedureCallInfo, Registry,
};
use super::super::{Command, CommandPrototype, CommandResult, LocationInfo};
use crate::machine::error::ErrorKind;
use crate::machine::{LoopCallInfo, Result};

pub const LOOP_COMMAND_NAMES: [&'static str; 2] = ["loop", "while"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Condition {
    None,
    BoolValue(bool),
}
impl Condition {
    pub fn evaluate(&self) -> bool {
        match self {
            Condition::BoolValue(b) => *b,
            Condition::None => false,
            _ => false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Loop {
    pub break_condition: Condition,
    pub start: usize,
    pub end: usize,
}

impl Loop {
    pub fn new(
        args: Vec<String>,
        location: &LocationInfo,
        commands: &Vec<CommandPrototype>,
    ) -> Result<Self> {
        let line = location.line.unwrap();

        // start names
        let mut start_names = Vec::new();
        start_names.extend(&LOOP_COMMAND_NAMES);
        // // middle names
        let mut middle_names = Vec::new();
        // middle_names.extend(&ELSE_COMMAND_NAMES);
        // TODO push middle_names as start_names?
        // start_names.append(&mut middle_names.clone());
        // end names
        let mut end_names = Vec::new();
        end_names.extend(&super::end::COMMAND_NAMES);
        // other block starting names
        let mut start_blocks = Vec::new();
        start_blocks.extend(&super::procedure::COMMAND_NAMES);
        start_blocks.extend(&super::forin::COMMAND_NAMES);
        // other block ending names
        let mut end_blocks = Vec::new();
        end_blocks.extend(&super::end::COMMAND_NAMES);

        let positions_opt = match crate::machine::command_search(
            location,
            &commands,
            (line + 1, None),
            (&start_names, &middle_names, &end_names),
            (&start_blocks, &end_blocks),
            true,
        ) {
            Ok(po) => po,
            Err(e) => {
                return Err(Error::new(
                    location.clone(),
                    ErrorKind::InvalidCommandBody(e.to_string()),
                ))
            }
        };

        // condition
        let condition = if !args.is_empty() {
            match args[0].as_str() {
                "true" => Condition::BoolValue(true),
                "false" => Condition::BoolValue(false),
                _ => unimplemented!(),
            }
        } else {
            Condition::None
        };

        match positions_opt {
            Some(positions) => Ok(Self {
                break_condition: condition,
                start: line,
                end: positions.0,
            }),
            None => Err(Error::new(
                location.clone(),
                ErrorKind::InvalidCommandBody("end of if/else block not found.".to_string()),
            )),
        }
    }
    pub fn execute_loc(&self, call_stack: &mut CallStackVec) -> CommandResult {
        let call_info = CallInfo::Loop(LoopCallInfo {
            start: self.start,
            end: self.end,
        });
        call_stack.push(call_info);
        CommandResult::Continue
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Break;

impl Break {
    pub fn execute_loc(
        &self,
        //call_stack: &mut Vec<CallInfo>,
        call_stack: &mut CallStackVec,
        // component: &mut Component,
        ent_storage: &mut Storage,
        location: &LocationInfo,
    ) -> CommandResult {
        debug!("execute break");
        let mut result = CommandResult::Continue;
        match call_stack.pop() {
            Some(call_info) => {
                if let CallInfo::Loop(ci) = &call_info {
                    CommandResult::JumpToLine(ci.end)
                } else {
                    // return the call info to the stack
                    call_stack.push(call_info);
                    CommandResult::Continue
                }
            }
            None => CommandResult::Err(Error::new(location.clone(), ErrorKind::StackEmpty)),
        }
        // result
        // CommandResult::Ok
    }
}
