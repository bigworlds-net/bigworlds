//! Glue module implementing actual use of the script processor by the program.
//!
//! This module is responsible for implementing functionality that involves
//! reading module files. This includes:
//! - specifying how `Sim` instance is actually created out of the underlying
//! scenario file hierarchy
//! - implementing ext execution function for `cmd::assembly::extend` command,
//! as it's also concerned with reading and parsing files into instructions
//! that are used to extend the component logic model

use fnv::FnvHashMap;
use std::path::PathBuf;
use std::str::FromStr;

use crate::{EntityId, EntityName, ShortString, StringId, Var, VarType};

use super::{parser, preprocessor, Instruction, InstructionType};

use crate::entity::Entity;
use crate::error::Error;
use crate::machine::cmd::{Command, CommandResult};
use crate::machine::{cmd, exec, CommandPrototype, ErrorKind, LocationInfo};
use crate::model::{EventModel, Model, PrefabModel, Var as VarModel};
use crate::{machine, Result};

pub const FILE_EXTENSION: &'static str = ".os";

impl Model {}

// impl cmd::register::Extend {
//     /// Extends the given component's model by whatever is found in the target
//     /// source files.
//     ///
//     /// For each given script source file, this command will read it,
//     /// apply preprocessor, and then execute found commands.
//     pub fn execute_ext(&self, mut model: &mut SimModel, ent_uid: &EntityId) -> machine::Result<()> {
//         //println!("execute ext extend");
//         // iterate over all the given source files
//         for file in &self.source_files {
//             //debug!("{:?}", file);
//             //TODO create the path from project_root + relative_location_path +
//             // arg_file_path let project_root = &sim.model.scenario.path;
//             let file_path = PathBuf::from_str(self.location.source.as_ref().unwrap())
//                 .unwrap()
//                 .parent()
//                 .unwrap()
//                 .join(file);
//             //let file_path = project_root.join(relative_path);
//             //debug!("{:?}", file_path);
//             let mut instructions =
//                 match super::parse_script_at(&file_path.to_str().unwrap(), &model.scenario.path) {
//                     Ok(i) => i,
//                     Err(_) => {
//                         return Err(machine::Error::new(
//                             LocationInfo::empty(),
//                             ErrorKind::ErrorReadingFile("blah".to_string()),
//                         ))
//                     }
//                 };
//
//             // run the preprocessor
//             preprocessor::run(
//                 &mut instructions,
//                 &mut model,
//                 &super::util::get_program_metadata(),
//             )?;
//
//             // get command prototypes
//             let mut cmd_prototypes: Vec<CommandPrototype> = Vec::new();
//             let mut cmd_locations: Vec<LocationInfo> = Vec::new();
//             for instruction in instructions {
//                 let location = instruction.location;
//                 let cmd_proto = match instruction.type_ {
//                     InstructionType::Command(c) => c,
//                     _ => continue,
//                 };
//                 cmd_prototypes.push(cmd_proto);
//                 cmd_locations.push(location);
//             }
//
//             let comp_uid = self.comp_signature.clone();
//             let mut comp_model = model.get_component_mut(&comp_uid).unwrap();
//
//             let offset = comp_model.logic.commands.len();
//             // create commands from prototypes, so far working only with source script line
//             // numbers
//             for (n, cmd_prototype) in cmd_prototypes.iter().enumerate() {
//                 let mut location = &mut cmd_locations[n];
//                 location.line = Some(n);
//                 // println!("{:?}", location.line);
//                 let mut command =
//                     Command::from_prototype(cmd_prototype, &location, &cmd_prototypes)?;
//
//                 // apply line offset to flow control commands so that all the line numbers
//                 // stored within the commands themselves match the new combined collection
//                 command.apply_offset(offset);
//
//                 if let Command::Procedure(proc) = &command {
//                     comp_model
//                         .logic
//                         .procedures
//                         .insert(proc.name.clone(), (proc.start_line, proc.end_line));
//                 }
//                 if let Command::State(state) = &command {
//                     comp_model
//                         .logic
//                         .states
//                         .insert(state.name.clone(), (state.start_line, state.end_line));
//                     debug!("inserted state at comp_model: {}", comp_model.name);
//                 }
//                 comp_model.logic.commands.push(command);
//                 location.line = Some(location.line.unwrap() + offset);
//                 comp_model
//                     .logic
//                     .cmd_location_map
//                     .insert(comp_model.logic.commands.len() - 1, location.clone());
//             }
//             let comp_model = comp_model.clone();
//
//             // for ent_uid in
//             // &sim.entities.keys().collect::<Vec<&(ShortString,ShortString)>>()
//             // {
//
//             // TODO
//             // // execute on entities of matching type
//             // for ent_uid in sim
//             //     .entities
//             //     .keys()
//             //     .filter(|(ent_type, _)| {
//             //         ent_type ==
//             // self.comp_signature.get_ent_type_safe().unwrap().as_str()
//             //     })
//             //     .map(|euid| euid.clone())
//             //     .collect::<Vec<EntityIndex>>()
//             // {
//             //     exec::execute(
//             //         &comp_model.logic.commands,
//             //         &ent_uid,
//             //         &comp_uid,
//             //         sim,
//             //         // we set offset value as starting position, so that we
//             //         // don't repeat processing top level commands
//             //         Some(offset),
//             //         None,
//             //     );
//             // }
//         }
//         Ok(())
//     }
// }

impl Command {
    /// Apply line offset to flow control commands.
    fn apply_offset(&mut self, offset: usize) {
        match self {
            Command::State(cmd) => {
                cmd.start_line += offset;
                cmd.end_line += offset;
            }
            Command::Procedure(cmd) => {
                cmd.start_line += offset;
                cmd.end_line += offset;
            }
            // Command::If(cmd) => {
            //     cmd.start += offset;
            //     cmd.end += offset;
            //     for el in &mut cmd.else_lines {
            //         *el += offset;
            //     }
            // }
            // Command::ForIn(cmd) => {
            //     cmd.start += offset;
            //     cmd.end += offset;
            // }
            _ => (),
        }
    }
}
