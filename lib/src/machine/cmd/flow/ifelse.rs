use smallvec::SmallVec;

use crate::address::ShortLocalAddress;
use crate::{address, string, Address, CompName, Var};

use crate::entity::{Entity, Storage};
use crate::model::Model;

use super::super::super::error::Error;
use super::super::super::{
    CallInfo, CallStackVec, ForInCallInfo, IfElseCallInfo, IfElseMetaData, ProcedureCallInfo,
    Registry,
};
use super::super::{Command, CommandPrototype, CommandResult, LocationInfo};
use crate::machine::error::ErrorKind;
use crate::machine::{Machine, Result};

pub const IF_COMMAND_NAMES: [&'static str; 1] = ["if"];
pub const ELSE_COMMAND_NAMES: [&'static str; 2] = ["else", "else_if"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Condition {
    // Command()
    AssertEqual((Address, Var)),
    BooleanCheck(Address),
    BooleanCheckLocal(ShortLocalAddress),
    BoolValue(bool),
}
impl Condition {
    pub async fn evaluate(&self, machine: &Machine, comp_name: &CompName) -> Result<bool> {
        match self {
            Condition::AssertEqual((addr, right)) => {
                let left = machine.get_var(addr.to_owned()).await?;
                if left == *right {
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Condition::BooleanCheck(addr) => Ok(machine.get_var(addr.clone()).await?.to_bool()),
            // Condition::LocalVarAddress(addr) => Ok(storage
            //     .get_var(&addr.storage_index(Some(comp_name.clone()))?)?
            //     .to_bool()
            //     == true),
            Condition::BoolValue(b) => Ok(*b),
            _ => Ok(false),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct If {
    pub condition: Condition,
    pub start: usize,
    pub end: usize,
    pub else_lines: SmallVec<[usize; 10]>,
}

impl If {
    pub fn new(
        args: Vec<String>,
        location: &LocationInfo,
        commands: &Vec<CommandPrototype>,
    ) -> Result<If> {
        if args.len() == 0 {
            return Err(Error::new(
                location.clone(),
                ErrorKind::InvalidCommandBody("no arguments provided".to_string()),
            ));
        }
        let line = location.line.unwrap();

        // start names
        let mut start_names = Vec::new();
        start_names.extend(&IF_COMMAND_NAMES);
        // middle names
        let mut middle_names = Vec::new();
        middle_names.extend(&ELSE_COMMAND_NAMES);
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

        let positions_opt = match super::super::super::command_search(
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

        println!("ifelse args: {:?}", args);
        let condition = if args[0].contains(address::SEPARATOR_SYMBOL) && args.len() == 1 {
            // Condition::LocalVarAddress(args[0].parse().unwrap())
            Condition::BooleanCheck(args[0].parse().unwrap())
        } else if args[1] == "==" {
            println!("making condition");
            Condition::AssertEqual((args[0].parse()?, Var::from_str(&args[2], None)?))
        } else {
            match args[0].as_str() {
                "true" => Condition::BoolValue(true),
                _ => Condition::BoolValue(false),
            }
        };
        println!("made condition");

        match positions_opt {
            Some(positions) => Ok(If {
                condition,
                start: line,
                end: positions.0,
                else_lines: SmallVec::from(positions.1),
            }),
            None => Err(Error::new(
                location.clone(),
                ErrorKind::InvalidCommandBody("end of if/else block not found.".to_string()),
            )),
        }
    }
    pub async fn execute(
        &self,
        machine: &mut Machine,
        call_stack: &mut CallStackVec,
        // ent_storage: &mut Storage,
        // comp_name: &CompName,
        location: &LocationInfo,
    ) -> CommandResult {
        let mut else_lines_arr = [0; 10];
        for (n, el) in self.else_lines.iter().enumerate() {
            else_lines_arr[n] = *el;
        }
        if self
            .condition
            .evaluate(machine, &string::new_truncate("todo"))
            .await
            .unwrap()
        {
            debug!("evaluated to true");
            let next_line = if self.else_lines.is_empty() {
                self.end
            } else {
                self.else_lines[0]
            };

            let call_info = CallInfo::IfElse(IfElseCallInfo {
                current: next_line,
                passed: true,
                else_line_index: 0,
                meta: IfElseMetaData {
                    start: self.start,
                    end: self.end,
                    else_lines: else_lines_arr,
                    //else_lines: self.else_lines.into_iter().collect::<[usize; 10]>(),
                },
            });
            call_stack.push(call_info);
            CommandResult::Continue
        } else {
            debug!("evaluated to false");
            if !self.else_lines.is_empty() {
                let goto_line = self.else_lines[0];
                let call_info = CallInfo::IfElse(IfElseCallInfo {
                    current: goto_line,
                    passed: false,
                    else_line_index: 0,
                    meta: IfElseMetaData {
                        start: self.start,
                        end: self.end,
                        else_lines: else_lines_arr,
                    },
                });
                call_stack.push(call_info);
                CommandResult::JumpToLine(goto_line)
            } else {
                let goto_line = self.end + 1;
                CommandResult::JumpToLine(goto_line)
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ElseIf {
    condition: Condition,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Else {}
impl Else {
    pub fn new(args: Vec<String>) -> Result<Else> {
        Ok(Else {})
    }
    pub fn execute_loc(
        &self,
        //call_stack: &mut Vec<CallInfo>,
        call_stack: &mut CallStackVec,
        // component: &mut Component,
        ent_storage: &mut Storage,
        location: &LocationInfo,
    ) -> CommandResult {
        debug!("execute else");
        let mut result = CommandResult::Continue;
        match call_stack.pop() {
            Some(call_info) => {
                match &call_info {
                    CallInfo::IfElse(ci) => {
                        if ci.passed {
                            let goto_line = ci.meta.end + 1;
                            result = CommandResult::JumpToLine(goto_line);
                        }
                    }
                    _ => (),
                }
                // return the call info to the stack
                call_stack.push(call_info);
            }
            None => {
                result = CommandResult::Err(Error::new(location.clone(), ErrorKind::StackEmpty));
            }
        }
        result
        // CommandResult::Ok
    }
}
