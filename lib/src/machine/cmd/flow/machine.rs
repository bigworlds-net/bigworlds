use crate::machine::cmd::Command;
use crate::machine::error::{Error, ErrorKind, Result};
use crate::machine::{CommandPrototype, LocationInfo};
use crate::{string, Address, CompName, StringId};

pub const COMMAND_NAMES: [&'static str; 1] = ["machine"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct MachineBlock {
    // pub name: StringId,
    pub start_line: usize,
    pub end_line: usize,
    pub output_variable: Option<Address>,
}

impl MachineBlock {
    pub fn new(
        args: Vec<String>,
        location: &LocationInfo,
        commands: &Vec<CommandPrototype>,
    ) -> Result<Command> {
        trace!("creating new machine block");

        let line = location.line.unwrap();

        // start names
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
        start_blocks.extend(&super::state::COMMAND_NAMES);
        // other block ending names
        let mut end_blocks = Vec::new();
        end_blocks.extend(&super::end::COMMAND_NAMES);

        match crate::machine::command_search(
            location,
            &commands,
            (line + 1, None),
            (&start_names, &middle_names, &end_names),
            (&start_blocks, &end_blocks),
            true,
        ) {
            Ok(positions_options) => match positions_options {
                Some(positions) => Ok(Command::Machine(MachineBlock {
                    // name: string::new_truncate(&args[0]),
                    // source_comp: location.comp_name.clone().unwrap(),
                    // source_file: location.source.unwrap(),
                    start_line: line + 1,
                    end_line: positions.0,
                    output_variable: None,
                })),
                // {
                //     Ok(Command::Register(Register::Component(RegComponent {
                //         name: StringId::from_truncate(&args[0]),
                //         trigger_events: vec![],
                //     })))
                // }
                None => Err(Error::new(
                    location.clone(),
                    ErrorKind::InvalidCommandBody("end of component block not found".to_string()),
                )),
            },
            Err(e) => {
                return Err(Error::new(
                    location.clone(),
                    ErrorKind::InvalidCommandBody(e.to_string()),
                ))
            }
        }
    }
}
