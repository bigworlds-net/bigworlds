use std::iter::FromIterator;

use crate::address::Address;
use crate::entity::{Entity, Storage};
use crate::machine::error::ErrorKind;
use crate::machine::{ComponentCallInfo, Logic, Machine};
use crate::model::Model;
use crate::{string, CompName, EntityId, EntityName, ShortString, StringId};

use super::super::super::error::Error;
use super::super::super::{
    CallInfo, CallStackVec, IfElseCallInfo, IfElseMetaData, ProcedureCallInfo, Registry,
};
use super::super::{Command, CommandPrototype, CommandResult, LocationInfo};

pub const COMMAND_NAMES: [&'static str; 1] = ["state"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct State {
    // pub comp: CompName,
    // pub signature: Option<Address>,
    pub name: StringId,
    pub start_line: usize,
    pub end_line: usize,
    // pub output_variable: Option<Address>,
}

impl State {
    pub fn new(
        args: Vec<String>,
        location: &LocationInfo,
        // commands: &Vec<(CommandPrototype, LocationInfo)>,
        commands: &Vec<CommandPrototype>,
    ) -> Result<Command, Error> {
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
        start_blocks.extend(&super::procedure::COMMAND_NAMES);
        start_blocks.extend(&super::component::COMMAND_NAMES);
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
            Some(positions) => Ok(Command::State(State {
                // comp: string::new_truncate(""),
                // signature: None,
                name: string::new_truncate(&args[0]),
                start_line: line + 1,
                end_line: positions.0,
                // output_variable: None,
            })),
            None => Err(Error::new(
                location.clone(),
                ErrorKind::InvalidCommandBody("end of state block not found".to_string()),
            )),
        }
    }
    pub fn execute(
        &self,
        machine: &Machine,
        logic: &mut Logic,
        call_stack: &mut CallStackVec,
        // ent_uid: &EntityId,
        // comp_name: &CompName,
        location: &LocationInfo,
    ) -> CommandResult {
        CommandResult::JumpToLine(self.end_line + 1)
    }
}
