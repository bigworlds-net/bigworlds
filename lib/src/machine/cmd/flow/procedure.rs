use std::convert::From;
use std::iter::FromIterator;

use crate::{string, Address, ShortString};

use crate::entity::{Entity, Storage};
use crate::model::Model;

use super::super::super::error::Error;
use super::super::super::{
    CallInfo, CallStackVec, IfElseCallInfo, IfElseMetaData, ProcedureCallInfo, Registry,
};
use super::super::{CommandPrototype, CommandResult, LocationInfo};
use crate::machine::error::ErrorKind;
use crate::machine::Result;

pub const COMMAND_NAMES: [&'static str; 2] = ["proc", "procedure"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Procedure {
    pub name: ShortString,
    pub start_line: usize,
    pub end_line: usize,
    pub output_variable: Option<Address>,
}
impl Procedure {
    pub fn new(
        args: Vec<String>,
        location: &LocationInfo,
        commands: &Vec<CommandPrototype>,
    ) -> Result<Procedure> {
        let line = location.line.unwrap();

        // TODO all these names should probably be declared in a
        // better place start names
        let mut start_names = Vec::new();
        start_names.extend(&COMMAND_NAMES);
        // middle names
        let mut middle_names = Vec::new();
        // end names
        let mut end_names = Vec::new();
        end_names.extend(&super::end::COMMAND_NAMES);
        // other block starting names
        let mut start_blocks = Vec::new();
        start_blocks.extend(&super::ifelse::IF_COMMAND_NAMES);
        start_blocks.extend(&super::ifelse::ELSE_COMMAND_NAMES);
        start_blocks.extend(&super::forin::COMMAND_NAMES);
        start_blocks.extend(&super::state::COMMAND_NAMES);
        // other block ending names
        let mut end_blocks = Vec::new();
        end_blocks.extend(&super::end::COMMAND_NAMES);

        let positions_options = match super::super::super::command_search(
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

        match positions_options {
            Some(positions) => Ok(Procedure {
                name: args[0].parse().unwrap(),
                start_line: line,
                end_line: positions.0,
                output_variable: None,
            }),
            None => Err(Error::new(
                location.clone(),
                ErrorKind::InvalidCommandBody("End of procedure block not found".to_string()),
            )),
        }
    }
    pub fn execute(&self, call_stack: &mut CallStackVec, location: &LocationInfo) -> CommandResult {
        // call_stack.push(CallInfo::Procedure(ProcedureCallInfo {
        //     call_line: line,
        //     start_line: self.start_line,
        //     end_line: self.end_line,
        // }));
        CommandResult::JumpToLine(self.end_line + 1)
    }
}
