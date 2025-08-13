use std::iter::FromIterator;

use crate::address::Address;
use crate::entity::{Entity, Storage};
use crate::machine::cmd::register::{RegisterComponent, RegisterEntityPrefab};
use crate::machine::cmd::{Command, CommandPrototype, CommandResult, LocationInfo};
use crate::machine::error::{Error, ErrorKind, Result};
use crate::machine::script::parse_script_at;
use crate::machine::{
    CallInfo, CallStackVec, ComponentCallInfo, IfElseCallInfo, IfElseMetaData, Machine,
    ProcedureCallInfo, Registry,
};
use crate::model::{self, Model, Var};
use crate::{rpc, CompName, EntityId, EntityName, Executor, VarType};

pub const COMMAND_NAMES: [&'static str; 1] = ["component"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct ComponentBlock {
    pub name: String,
    pub source_file: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub output_variable: Option<Address>,
}

impl ComponentBlock {
    pub fn new(
        args: Vec<String>,
        location: &LocationInfo,
        commands: &Vec<CommandPrototype>,
    ) -> Result<Command> {
        trace!("making new comp block");

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

        let positions_options = match crate::machine::command_search(
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
            Some(positions) => Ok(Command::Component(ComponentBlock {
                name: args[0].to_owned(),
                source_file: location.source.clone(),
                start_line: line + 1,
                end_line: positions.0,
                output_variable: None,
            })),
            None => Err(Error::new(
                location.clone(),
                ErrorKind::InvalidCommandBody("end of component block not found".to_string()),
            )),
        }
    }

    pub async fn execute(
        &self,
        machine: &Machine,
        call_stack: &mut CallStackVec,
        // ent_uid: &EntityId,
        // comp_name: &CompName,
        // line: usize,
    ) -> CommandResult {
        println!("executing component block: {:?}", self);

        call_stack.push(CallInfo::Component(ComponentCallInfo {
            name: self.name.clone(),
            start_line: self.start_line,
            end_line: self.end_line,
            vars: vec![],
        }));

        CommandResult::Continue
    }
    // pub fn execute_ext(&self, model: &mut Model) -> Result<()> {
    //     trace!("registering component");

    //     let comp_model = model.get_component(&self.source_comp).unwrap();

    //     let component = model::Component {
    //         name: self.name.clone(),
    //         //start_state: arraystring::new_unchecked("start"),
    //         // triggers: vec![StringId::from_unchecked("step")],
    //         // logic: LogicModel {
    //         //     commands: comp_model.logic.commands.clone(),
    //         //     cmd_location_map: comp_model.logic.cmd_location_map.clone(),
    //         //     ..LogicModel::default()
    //         // },
    //         // logic: comp_model.logic.get_subset(self.start_line, self.end_line),
    //         ..model::Component::default()
    //     };

    //     debug!("adding new component to model: {:?}", component);

    //     // overwrite existing components with the same name by default
    //     if let Some(n) = model
    //         .components
    //         .iter()
    //         .enumerate()
    //         .find(|(_, c)| c.name == component.name)
    //         .map(|(n, _)| n)
    //     {
    //         model.components.remove(n);
    //     }
    //     model.components.push(component);
    //     // trace!("{:?}", self);
    //     Ok(())
    // }

    // pub fn execute_ext_distr(&self, model: &mut Model) -> Result<()> {
    //     warn!("component block");
    //     let comp_model = model.get_component(&self.source_comp).unwrap();

    //     let component = model::Component {
    //         name: self.name.clone(),
    //         //start_state: arraystring::new_unchecked("start"),
    //         // triggers: vec![StringId::from_unchecked("step")],
    //         // logic: LogicModel {
    //         //     commands: comp_model.logic.commands.clone(),
    //         //     cmd_location_map: comp_model.logic.cmd_location_map.clone(),
    //         //     ..LogicModel::default()
    //         // },
    //         // logic: comp_model.logic.get_subset(self.start_line, self.end_line),
    //         ..model::Component::default()
    //     };

    //     trace!(
    //         "execute_ext_distr: adding new component to model: {:?}",
    //         component
    //     );

    //     // overwrite existing components with the same name by default
    //     if let Some(n) = model
    //         .components
    //         .iter()
    //         .enumerate()
    //         .find(|(_, c)| c.name == component.name)
    //         .map(|(n, _)| n)
    //     {
    //         model.components.remove(n);
    //     }
    //     model.components.push(component);
    //     // trace!("{:?}", self);
    //     Ok(())
    // }
}
