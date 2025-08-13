use smallvec::SmallVec;

use crate::address::{Address, LocalAddress, ShortLocalAddress};
use crate::{CompName, Int, Var, VarType};

use crate::entity::{Entity, Storage, StorageIndex};
use crate::model::Model;

use super::super::{Command, CommandPrototype, CommandResult, LocationInfo};
use crate::machine::cmd::flow::{end, ifelse};
use crate::machine::error::{Error, ErrorKind};
use crate::machine::{
    command_search, CallInfo, CallStackVec, ForInCallInfo, IfElseCallInfo, IfElseMetaData,
    ProcedureCallInfo, Registry, Result,
};

pub const COMMAND_NAMES: [&'static str; 1] = ["for"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct ForIn {
    pub start: usize,
    pub end: usize,
    pub target: ShortLocalAddress,
    pub variable: ShortLocalAddress,
}
impl ForIn {
    pub fn new(
        args: Vec<String>,
        location: &LocationInfo,
        commands: &Vec<CommandPrototype>,
    ) -> Result<ForIn> {
        let line = location.line.unwrap();

        let variable = match &args.get(0) {
            Some(arg) => ShortLocalAddress::from_str(arg).map_err(|e| {
                Error::new(location.clone(), ErrorKind::InvalidAddress(e.to_string()))
            })?,
            // Some(arg) => StringId::from(arg).unwrap(),
            None => {
                return Err(Error::new(
                    location.clone(),
                    ErrorKind::InvalidCommandBody(format_err_no_arguments(location)),
                ))
            }
        };
        let target = match args.get(2) {
            Some(arg) => ShortLocalAddress::from_str(arg).map_err(|e| {
                Error::new(location.clone(), ErrorKind::InvalidAddress(e.to_string()))
            })?,
            None => {
                return Err(Error::new(
                    location.clone(),
                    ErrorKind::InvalidCommandBody("too few arguments?".to_string()),
                ))
            }
        };

        // start names
        // TODO all these names should probably be declared in a better place
        let mut start_names = Vec::new();
        start_names.extend(&COMMAND_NAMES);
        // end names
        let mut end_names = Vec::new();
        end_names.extend(&super::end::COMMAND_NAMES);
        // other block starting names
        let mut start_blocks = Vec::new();
        start_blocks.extend(&super::ifelse::IF_COMMAND_NAMES);
        start_blocks.extend(&super::ifelse::ELSE_COMMAND_NAMES);
        start_blocks.extend(&COMMAND_NAMES);
        start_blocks.extend(&super::procedure::COMMAND_NAMES);
        // other block ending names
        let mut end_blocks = Vec::new();
        end_blocks.extend(&super::end::COMMAND_NAMES);

        let positions_options = match command_search(
            location,
            &commands,
            (line + 1, None),
            (&start_names, &Vec::new(), &end_names),
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

        //// condition
        //let condition = match args[0].as_str() {
        //"true" => Condition::BoolValue(true),
        //_ => Condition::BoolValue(false),
        //};

        match positions_options {
            Some(positions) => Ok(ForIn {
                start: line,
                end: positions.0,
                target,
                variable,
            }),
            None => Err(Error::new(
                location.clone(),
                ErrorKind::InvalidCommandBody("End of forin block not found.".to_string()),
            )),
        }
    }

    pub fn execute_loc(
        &self,
        call_stack: &mut CallStackVec,
        registry: &mut Registry,
        comp_id: &CompName,
        ent_storage: &mut Storage,
        location: &LocationInfo,
    ) -> CommandResult {
        // get target len
        // let iter_target = match ent_storage.get_var_from_addr(&self.target,
        // Some(comp_uid)) {
        let iter_target =
            match ent_storage.get_var(&self.target.as_storage_index_using(comp_id.clone())) {
                Ok(var) => var,
                Err(_) => {
                    return CommandResult::Err(Error::new(
                        location.clone(),
                        //todo
                        ErrorKind::FailedGettingFromStorage(self.target.to_string()),
                    ));
                }
            };

        let len = match iter_target {
            Var::Int(num) => *num as usize,
            Var::Float(num) => *num as usize,
            // Var::StringList(list) => list.len(),
            // Var::IntList(list) => list.len(),
            // Var::FloatList(list) => list.len(),
            // Var::BoolList(list) => list.len(),
            // #[cfg(feature = "grids")]
            // Var::StringGrid(list) => list.len(),
            _ => 0,
        };

        // let (comp_model, comp_id) = comp_uid;
        // let variable = (*comp_uid, self.variable.var_id);
        let variable = LocalAddress {
            // comp: self.variable.comp.unwrap_or(*comp_id),
            comp: self.variable.comp.clone().unwrap_or(comp_id.clone()),
            var_type: self.variable.var_type,
            var_name: self.variable.var_name.clone(),
        };
        let target = LocalAddress {
            // comp: self.target.comp.unwrap_or(*comp_id),
            comp: self.target.comp.clone().unwrap_or(comp_id.clone()),
            var_type: self.target.var_type,
            var_name: self.target.var_name.clone(),
        };
        // let target = (*comp_id, self.target.var_id);
        // let variable_type = self.variable.var_type;
        // ForIn::update_variable(&variable, &Some(variable_type), &target, 0,
        // ent_storage);
        ForIn::update_variable(&variable, &target, 0);

        // warn!("forin start");
        let call_info = CallInfo::ForIn(ForInCallInfo {
            target: Some(target),
            target_len: len,
            variable: Some(variable),
            // variable_type: Some(variable_type),
            iteration: 1,
            start: self.start,
            end: self.end,
        });
        call_stack.push(call_info);
        CommandResult::Continue
    }

    pub fn update_variable(
        variable: &LocalAddress,
        // variable_type: &Option<VarType>,
        target: &LocalAddress,
        iteration: usize,
        // ent_storage: &mut Storage,
    ) {
        match variable.var_type {
            VarType::Int => {
                match target.var_type {
                    // TODO
                    // VarType::Int => {
                    //     if let Ok(int_var) = ent_storage
                    //         .get_var_mut(&target.storage_index())
                    //         .unwrap()
                    //         .as_int_mut()
                    //     {
                    //         *int_var = iteration as Int;
                    //     }
                    // }

                    // VarType::IntList => {
                    //     let newvar = ent_storage
                    //         .get_var_mut(&target.storage_index())
                    //         .unwrap()
                    //         .as_int_list_mut()
                    //         .unwrap()[iteration];
                    //     match ent_storage
                    //         .get_var_mut(&variable.storage_index())
                    //         .unwrap()
                    //         .as_int_mut()
                    //     {
                    //         Ok(var) => *var = newvar,
                    //         Err(_) => {
                    //             ent_storage.insert(
                    //                 variable.storage_index(),
                    //                 Var::Int(
                    //                     ent_storage
                    //                         .get_var(&target.storage_index())
                    //                         .unwrap()
                    //                         .as_int_list()
                    //                         .unwrap()[iteration],
                    //                 ),
                    //             );
                    //         }
                    //     }
                    // }
                    _ => unimplemented!(),
                }

                // *ent_storage.int.get_mut(&variable).unwrap() =
                //     ent_storage.int_list.get(target).unwrap()[iteration];
            }
            _ => (),
        }
    }
}

use annotate_snippets::display_list::{DisplayList, FormatOptions};
use annotate_snippets::snippet::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::str::FromStr;

fn format_err_no_arguments(location: &LocationInfo) -> String {
    let start_line = location.source_line.unwrap();
    let mut source_file = File::open(location.source.as_ref().unwrap().to_string()).unwrap();
    let source_string: String = BufReader::new(source_file)
        .lines()
        .nth(start_line - 1)
        .unwrap()
        .unwrap();

    let range_start = 0;
    let range_end = source_string.len();

    let snippet = Snippet {
        title: Some(Annotation {
            label: Some("no arguments provided"),
            id: None,
            annotation_type: AnnotationType::Error,
        }),
        footer: vec![Annotation {
            label: Some("try adding more arguments"),
            id: None,
            annotation_type: AnnotationType::Help,
        }],
        slices: vec![Slice {
            source: &source_string,
            line_start: start_line,
            origin: Some(location.source.as_ref().unwrap()),
            fold: true,
            annotations: vec![SourceAnnotation {
                label: "`for` command requires additional arguments",
                annotation_type: AnnotationType::Error,
                range: (range_start, range_end),
            }],
        }],
        opt: FormatOptions {
            color: true,
            ..Default::default()
        },
    };

    let dl = DisplayList::from(snippet);
    format!("{}", dl)
}
