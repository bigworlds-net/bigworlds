//! Preprocessor is concerned with executing script directives.

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use crate::model::Model;

use super::{DirectivePrototype, Instruction, InstructionType};

use crate::machine::error::Error;
use crate::machine::{ErrorKind, LocationInfo, Result};

/// Runs full preprocessing pass on the given set of instructions.
///
/// Takes in an additional `data_table` that can be used by preprocessor
/// directives.
pub(crate) fn run(
    instructions: &mut Vec<Instruction>,
    sim_model: &mut Model,
    data_table: &HashMap<String, String>,
) -> Result<()> {
    // start by removing empty instructions
    eliminate_empty(instructions)?;
    // run include directives
    run_includes(instructions, sim_model)?;
    // run conditionals, eliminating subset of instructions
    run_conditionals(instructions)?;
    // run the remaining directives
    run_remaining(instructions, sim_model, data_table)?;
    // remove directives, leaving only command instructions
    eliminate_directives(instructions)?;
    Ok(())
}

/// Removes empty instructions.
fn eliminate_empty(instructions: &mut Vec<Instruction>) -> Result<()> {
    let mut out_instructions = Vec::new();
    for instruction in instructions.iter() {
        match instruction.type_ {
            InstructionType::None => continue,
            _ => out_instructions.push(instruction.clone()),
        }
    }
    *instructions = out_instructions;
    Ok(())
}
/// Removes directive instructions.
fn eliminate_directives(instructions: &mut Vec<Instruction>) -> Result<()> {
    let mut out_instructions = Vec::new();
    for instruction in instructions.iter() {
        match instruction.type_ {
            InstructionType::Directive(_) => continue,
            _ => out_instructions.push(instruction.clone()),
        }
    }
    *instructions = out_instructions;
    Ok(())
}

/// Runs all include directives, inserting new instruction sets parsed from
/// specified files into the main instruction set.
///
/// # File paths
/// Paths to included files are resolved based on the location info bound to
/// the processed instruction. In short file paths specified in include
/// directives are written as relative to the file where the directive is
/// present.
fn run_includes(instructions: &mut Vec<Instruction>, sim_model: &Model) -> Result<()> {
    // run until there are no include directives left
    while instructions.iter().any(|i| match &i.type_ {
        InstructionType::Directive(dp) => match &dp.name {
            Some(n) => n == "include",
            _ => false,
        },
        _ => false,
    }) {
        // get the next include directive
        let mut next_incl = None;
        let mut next_location = LocationInfo::empty();
        for (n, instr) in instructions.iter().enumerate() {
            match &instr.type_ {
                InstructionType::Directive(dp) => match &dp.name {
                    Some(i) => {
                        if i == "include" {
                            next_incl = Some(n);
                            next_location = instr.location.clone();
                        }
                    }
                    _ => continue,
                },
                _ => continue,
            }
        }
        match next_incl {
            Some(ni) => {
                // remove the directive from the main instruction list
                let incl = instructions.remove(ni);
                // get the path of
                let script_path = PathBuf::from_str(&incl.location.source.unwrap()).unwrap();
                let script_parent_path = script_path.parent().unwrap();
                // get the directive prototype
                let incl_proto = match incl.type_ {
                    InstructionType::Directive(dp) => dp,
                    _ => continue,
                };
                trace!("execute include: {:?}", incl_proto);
                // iterate over the arguments, which are the files to
                // include
                match incl_proto.arguments {
                    Some(args) => {
                        for incl_file in args {
                            // don't allow recursive includes
                            if &incl_file == script_path.file_name().unwrap().to_str().unwrap() {
                                trace!(
                                    "recursive !include detected, removing: {:?}",
                                    next_location
                                );
                                continue;
                            }
                            // parse the file at path
                            // TODO
                            // let new_instructions = super::parser::parse_script_at(
                            //     script_parent_path.join(incl_file).to_str().unwrap(),
                            //     &sim_model.scenario.path,
                            // )?;
                            let new_instructions = Vec::new();
                            instructions.splice(ni..ni, new_instructions);
                        }
                    }
                    None => warn!("include directive without arguments"),
                }
            }
            // no include directives found (shouldn't happen since we're already
            // inside a while loop which runs only if there are some left)
            None => continue,
        }
    }
    Ok(())
}

/// Processes the conditional directives, removing instructions that are inside
/// the conditional blocks that are evaluated to false.
fn run_conditionals(instructions: &mut Vec<Instruction>) -> Result<()> {
    let mut out_instructions = Vec::new();
    let mut inside_if_if = false;
    let mut inside_if_else = false;

    for instruction in instructions.iter() {
        match &instruction.type_ {
            InstructionType::Command(cmd) => {
                // omit commands inside any if block
                if !inside_if_if && !inside_if_else {
                    out_instructions.push(instruction.clone())
                }
            }
            InstructionType::Directive(directive) => {
                match directive.name.as_ref().unwrap().as_str() {
                    "if" => {
                        if inside_if_if {
                            return Err(Error::new(
                                instruction.location.clone(),
                                ErrorKind::ErrorProcessingDirective(
                                    "nested `if` directives not supported".to_string(),
                                ),
                            ));
                        } else if inside_if_else {
                            return Err(Error::new(
                                instruction.location.clone(),
                                ErrorKind::ErrorProcessingDirective(
                                    "cannot place `if` inside `else` block".to_string(),
                                ),
                            ));
                        } else {
                            inside_if_if = true;
                        }
                    }
                    "else" => {
                        if inside_if_if {
                            inside_if_if = false;
                            inside_if_else = true;
                        } else if inside_if_else {
                            return Err(Error::new(
                                instruction.location.clone(),
                                ErrorKind::ErrorProcessingDirective(
                                    "nested `else` directives are not allowed".to_string(),
                                ),
                            ));
                        } else {
                            return Err(Error::new(
                                instruction.location.clone(),
                                ErrorKind::ErrorProcessingDirective(
                                    "`else` must be declared inside `if` block".to_string(),
                                ),
                            ));
                        }
                    }
                    "endif" => {
                        if inside_if_if {
                            inside_if_if = false;
                        } else if inside_if_else {
                            inside_if_else = false;
                        } else {
                            return Err(Error::new(
                                instruction.location.clone(),
                                ErrorKind::ErrorProcessingDirective(
                                    "`endif` without preceding `if`".to_string(),
                                ),
                            ));
                        }
                    }
                    _ => {
                        // TODO evaluate contitional statements
                        if inside_if_if {
                            // out_instructions.
                            // push(instruction.clone());
                        } else if inside_if_else {
                            out_instructions.push(instruction.clone());
                        } else {
                            // evaluate conditional statement
                            //
                            out_instructions.push(instruction.clone());
                        }
                    }
                }
            }
            _ => continue,
        }
    }
    *instructions = out_instructions;
    Ok(())
}

fn run_remaining(
    instructions: &mut Vec<Instruction>,
    sim_model: &mut Model,
    program_data: &HashMap<String, String>,
) -> Result<()> {
    for instruction in instructions {
        match &instruction.type_ {
            InstructionType::Directive(directive) => {
                match directive.name.as_ref().unwrap().as_str() {
                    "print" => run_print(&directive, program_data, &instruction.location)?,
                    _ => continue,
                }
            }
            _ => continue,
        }
    }
    Ok(())
}

fn run_print(
    directive: &DirectivePrototype,
    program_data: &HashMap<String, String>,
    location: &LocationInfo,
) -> Result<()> {
    let mut print_string = directive.arguments.as_ref().unwrap()[0].to_string();

    loop {
        if let Some(start_index) = print_string.find("${") {
            if let Some(end_index) = print_string.find("}") {
                print_string = format!(
                    "{}{}{}",
                    &print_string[..start_index],
                    match program_data.get(&print_string[start_index + 2..end_index]) {
                        Some(val) => val,
                        None => "ERROR",
                    },
                    &print_string[end_index + 1..],
                );
            } else {
                break;
            }
        } else if let Some(start_index) = print_string.find("$") {
            if let Some(end_index) = print_string[start_index..].find(" ") {
                print_string = format!(
                    "{}{}{}",
                    &print_string[..start_index],
                    match program_data.get(&print_string[start_index + 1..end_index]) {
                        Some(val) => val,
                        //None => error!("error"); "ERROR",
                        None => {
                            return Err(Error::new(
                                location.clone(),
                                ErrorKind::ErrorProcessingDirective(format!(
                                    "no data: {}",
                                    &print_string[start_index + 1..end_index],
                                )),
                            ));
                        }
                    },
                    &print_string[end_index..],
                );
            } else {
                print_string = format!(
                    "{}{}",
                    &print_string[..start_index],
                    match program_data.get(&print_string[start_index + 1..]) {
                        Some(val) => val,
                        None => {
                            return Err(Error::new(
                                location.clone(),
                                ErrorKind::ErrorProcessingDirective(format!(
                                    "no data: {}",
                                    &print_string[start_index + 1..]
                                )),
                            ));
                        }
                    },
                );
            }
        }
        //if print_string.contains("$") {
        //let start_index = print_string.find("$").
        //}
        else {
            break;
        }
    }

    info!("[preprocessor] {}", print_string);
    Ok(())
}

pub enum Directive {
    Print(Print),
    //IncludeScript(IncludeScript),
    //ExpandComponent(ExpandComponent),
    //IfElse(IfElse),
}

impl Directive {
    pub fn new(name: String, args: Vec<String>) -> Directive {
        unimplemented!();
        // match &name {
        //"print" => Print::new
        //}
    }
    pub fn exec(&self) {
        match self {
            Directive::Print(dir) => dir.exec(),
            _ => (),
        }
    }
}

trait ExecDirective {}

pub struct Print {
    body: String,
}
impl Print {
    pub fn exec(&self) {
        info!("{}", self.body);
    }
}
