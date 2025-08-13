//! This module defines commands used for assembling the model at runtime.
//! This is done by registering different parts of the model one at a time.

use std::path::PathBuf;
use std::str::FromStr;

use shlex::Shlex;

use crate::address::{Address, LocalAddress, ShortLocalAddress};
use crate::{model, rpc, CompName, EntityId, Float, Var};

use crate::entity::Storage;
use crate::executor::{Executor, Signal};
use crate::model::{Component, EventModel, Model, PrefabModel};

use super::super::script::parse_script_at;
use super::super::LocationInfo;
use super::CommandResult;
use crate::machine::cmd::flow::component::ComponentBlock;
use crate::machine::cmd::Command;
use crate::machine::error::{Error, ErrorKind, Result};
use crate::machine::{self, Machine};
use crate::machine::{CallInfo, CallStackVec, CommandPrototype, ComponentCallInfo};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct RegisterVar {
    comp: CompName,
    addr: ShortLocalAddress,
    val: Option<Var>,
}

impl RegisterVar {
    pub fn new(args: Vec<String>, location: &LocationInfo) -> Result<Self> {
        let mut options = getopts::Options::new();
        options.optopt("", "default", "", "");

        let matches = options.parse(&args).unwrap();

        if matches.free.is_empty() {
            return Err(Error::new(
                location.clone(),
                ErrorKind::InvalidCommandBody(format!("Must provide variable address as free arg")),
            ));
        }

        let val = if let Ok(Some(default)) = matches.opt_get::<Float>("default") {
            Some(Var::Float(default))
        } else {
            None
        };

        let addr = match ShortLocalAddress::from_str(&matches.free[0]) {
            Ok(a) => a,
            Err(e) => {
                return Err(Error::new(
                    location.clone(),
                    ErrorKind::InvalidCommandBody(format!("{}", e)),
                ))
            }
        };

        Ok(RegisterVar {
            comp: CompName::new(),
            addr,
            val,
        })
    }

    pub async fn execute(&self, machine: &Machine, call_stack: &mut CallStackVec) -> CommandResult {
        let mut new_reg_var = self.clone();
        if let Some(call_info) = call_stack.last_mut() {
            if let CallInfo::Component(comp_call_info) = call_info {
                comp_call_info.vars.push(model::Var {
                    name: self.addr.var_name.clone(),
                    type_: self.addr.var_type,
                    default: self.val.clone(),
                });
            }
        }
        CommandResult::Continue
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Extend {
    // args: Vec<String>,
    /// Partial address acting as a signature for target component,
    /// including entity type but not the entity id
    pub(crate) comp_signature: String,
    pub(crate) source_files: Vec<String>,
    pub(crate) location: LocationInfo,
}

impl Extend {
    pub fn new(args: Vec<String>, location: &LocationInfo) -> Result<Self> {
        if args.len() < 2 {
            return Err(Error::new(
                location.clone(),
                ErrorKind::InvalidCommandBody(
                    "`extend` command requires at least 2 arguments".to_string(),
                ),
            ));
        }
        let comp_signature = args[0].to_owned();
        let mut source_files = Vec::new();
        for i in 1..args.len() {
            // check for potential recursion and abort if present
            if &args[i]
                == location
                    .source
                    .as_ref()
                    .unwrap()
                    .rsplitn(2, "/")
                    .collect::<Vec<&str>>()[0]
            {
                trace!("detected recursive !extend, removing: {:?}", location);
                continue;
            }
            source_files.push(args[i].clone());
        }
        return Ok(Extend {
            comp_signature,
            source_files,
            location: location.clone(),
        });
    }
    pub fn execute(&self) -> CommandResult {
        unimplemented!()
        // CommandResult::ExecCentralExt(CentralRemoteCommand::Extend(self.clone()))
    }
}

/// Register an entity prefab, specifying a name and a set of components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterEvent {
    /// Name of the event
    name: String,
}

impl RegisterEvent {
    pub fn new(args: Vec<String>, location: &LocationInfo) -> Result<Self> {
        Ok(Self {
            name: args[0].to_owned(),
        })
    }

    pub fn execute(&self) -> CommandResult {
        info!("registering event");
        // CommandResult::ExecCentralExt(CentralRemoteCommand::RegisterEvent(Self {
        //     name: self.name.clone(),
        // }));
        CommandResult::Continue
    }
}

/// Register an entity prefab, specifying a name and a set of components.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct RegisterEntityPrefab {
    /// Name of the entity prefab
    name: String,
    /// List of components defining the prefab
    components: Vec<String>,

    setters: Vec<(LocalAddress, Var)>,
}

impl RegisterEntityPrefab {
    pub fn new(args: Vec<String>, location: &LocationInfo) -> Result<Self> {
        let mut options = getopts::Options::new();
        options.optopt("c", "component", "", "");
        options.optopt("", "set", "", "");
        let matches = options.parse(&args)?;
        Ok(Self {
            name: args[0].to_owned(),
            components: matches.opt_strs("component"),
            setters: matches
                .opt_strs("set")
                .into_iter()
                .map(|s| {
                    s.split_once("=")
                        .map(|(a, b)| {
                            (
                                LocalAddress::from_str(a).unwrap(),
                                Var::from_str(b, None).unwrap(),
                            )
                        })
                        .unwrap()
                })
                .collect::<Vec<_>>(),
        })
    }

    pub async fn execute(&self, machine: &Machine) -> CommandResult {
        let mut model = Model::default();
        model.prefabs.push(PrefabModel {
            name: self.name.clone(),
            components: self.components.clone(),
        });
        let _ = machine
            .worker
            .execute(Signal::from(rpc::worker::Request::MergeModel(model)))
            .await
            .unwrap();
        CommandResult::Continue
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct RegisterComponent {
    pub name: String,
    // pub trigger_events: Vec<StringId>,
    // pub source_comp: StringId,
    // pub start_line: usize,
    // pub end_line: usize,
}

impl RegisterComponent {
    pub fn new(
        args: Vec<String>,
        location: &LocationInfo,
        commands: &Vec<CommandPrototype>,
    ) -> Result<Command> {
        // println!("RegisterComponent::new: {:?}", args);
        // let mut options = getopts::Options::new();
        // let matches = options.parse(&args).unwrap();
        // if !matches.opt_count() {
        //     println!("not component block");
        //     Ok(Command::RegisterComponent(Self {
        //         name: string::new_truncate(&matches.free[0]),
        //     }))
        // } else {
        //     println!("component block");
        // }

        // HACK: for now disallow `component` oneliners and treat all
        // invocations as component blocks.
        ComponentBlock::new(args, location, commands)
    }

    pub async fn execute(&self, machine: &Machine) -> CommandResult {
        machine
            .worker
            .execute(Signal::from(rpc::worker::Request::MergeModel(Model {
                components: vec![Component {
                    name: self.name.clone(),
                    vars: vec![],
                }],
                ..Default::default()
            })))
            .await;

        CommandResult::Continue
    }

    pub fn execute_ext_distr(&self, model: &mut Model) -> Result<()> {
        let component = model::Component {
            name: self.name.clone(),
            // triggers: self.trigger_events.clone(),
            ..model::Component::default()
        };
        trace!(
            "execute_ext_distr: registering component: {:?}",
            component.name
        );
        model.components.push(component);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct RegisterTrigger {
    pub name: String,
    pub comp: CompName,
}

impl RegisterTrigger {
    pub fn new(args: Vec<String>, location: &LocationInfo) -> Result<Self> {
        // TODO handle cases with wrong number of args

        Ok(RegisterTrigger {
            name: args[0].to_owned(),
            comp: Default::default(),
        })
    }

    pub fn execute(&self, call_stack: &mut CallStackVec) -> CommandResult {
        // println!("call stack: {:?}", call_stack);
        let mut new_reg_trigger = self.clone();
        if let Some(comp_info) = call_stack.iter().find_map(|ci: &CallInfo| match ci {
            CallInfo::Component(c) => Some(c),
            _ => None,
        }) {
            new_reg_trigger.comp = comp_info.name.clone();
            debug!("Register::Trigger: comp_info.name: {}", comp_info.name);
        } else {
            // error: called outside of component block
        }

        // CommandResult::ExecCentralExt(CentralRemoteCommand::RegisterTrigger(new_reg_trigger)),
        CommandResult::Continue
    }
}

// impl Register {
//     pub fn new(args: Vec<String>, location: &LocationInfo) -> Result<Self> {
//         let mut options = getopts::Options::new();
//         let cmd_name = "register";
//
//         let reg = match args[0].as_str() {
//             "entity" => {}
//             "component" => {
//                 let brief = format!("usage: {} component <signature>
// [options]", cmd_name);                 options.optopt(
//                     "t",
//                     "trigger",
//                     "list of events that will trigger processing of this
// component",                     "EVENTS",
//                 );
//                 options.optflag(
//                     "a",
//                     "attach",
//                     "whether to attach the component when applying model",
//                 );
//                 let matches = match options.parse(&args[1..]) {
//                     Ok(m) => m,
//                     Err(e) => {
//                         return Err(Error::new(
//                             *location,
//                             ErrorKind::InvalidCommandBody(format!(
//                                 "{}, {}",
//                                 e,
//                                 options.usage(&brief)
//                             )),
//                         ))
//                     }
//                 };
//                 if matches.free.len() < 1 {
//                     return Err(Error::new(
//                         *location,
//                         ErrorKind::InvalidCommandBody(format!(
//                             "{}, {}",
//                             "signature missing",
//                             options.usage(&brief)
//                         )),
//                     ));
//                 }
//                 let trigger_events: Vec<StringId> = match
// matches.opt_str("trigger") {                     Some(str) => str
//                         .split(',')
//                         .map(|s| StringId::from_truncate(s))
//                         .collect::<Vec<StringId>>(),
//                     None => Vec::new(),
//                 };
//                 RegisterComponent {
//                     name: StringId::from_truncate(&matches.free[0]),
//                     trigger_events,
//                     source_comp: Default::default(),
//                     start_line: 0,
//                     end_line: 0,
//                 }
//             }
//             "event" => Register::Event,
//             "var" => Register::Var(RegisterVar {
//                 comp: CompId::new(),
//                 addr: LocalAddress::from_str(&args[1]).unwrap(),
//                 val: None,
//             }),
//             "trigger" => Register::Trigger(RegisterTrigger {
//                 name: StringId::from_truncate(&args[1]),
//                 comp: CompId::new(),
//             }),
//             _ => {
//                 return Err(Error::new(
//                     *location,
//                     ErrorKind::InvalidCommandBody("invalid register
// kind".to_string()),                 ))
//             }
//         };
//         Ok(reg)
//     }
//
//     pub fn execute_loc(&self, call_stack: &mut CallStackVec) ->
// Vec<CommandResult> {         let mut out_vec = Vec::new();
//         match &self {
//             // Register::Entity()
//             Register::Component(reg_comp) => {
//                 out_vec.push(CommandResult::ExecCentralExt(
//                     CentralRemoteCommand::Register(self.clone()),
//                 ));
//             }
//             Register::Var(reg_var) => {
//                 let mut new_reg_var = reg_var.clone();
//                 if let Some(comp_info) = call_stack.iter().find_map(|ci:
// &CallInfo| match ci {                     CallInfo::Component(c) => Some(c),
//                     _ => None,
//                 }) {
//                     new_reg_var.comp = comp_info.name;
//                     // debug!("comp_info.name: {}", comp_info.name);
//                 }
//                 out_vec.push(CommandResult::ExecCentralExt(
//
// CentralRemoteCommand::Register(Register::Var(new_reg_var)),
// ));             }
//             Register::Trigger(reg_trigger) => {
//                 let mut new_reg_trigger = reg_trigger.clone();
//                 if let Some(comp_info) = call_stack.iter().find_map(|ci:
// &CallInfo| match ci {                     CallInfo::Component(c) => Some(c),
//                     _ => None,
//                 }) {
//                     new_reg_trigger.comp = comp_info.name;
//                     debug!("Register::Trigger: comp_info.name: {}",
// comp_info.name);                 }
//                 out_vec.push(CommandResult::ExecCentralExt(
//
// CentralRemoteCommand::Register(Register::Trigger(new_reg_trigger)),
//                 ));
//             }
//             //     RegisterKind::Entity(ref mut reg) =>
// reg.signature.resolve_loc(storage),             _ => (),
//         }
//         out_vec.push(CommandResult::Continue);
//         return out_vec;
//         // println!("{:?}", self);
//     }
//     pub fn execute_ext(
//         &self,
//         sim: &mut Sim,
//         ent_name: &crate::EntityId,
//         comp_name: &crate::CompId,
//     ) -> Result<()> {
//         match &self {
//             Register::Entity(reg) => {
//                 // debug!("registering entity");
//                 // let signature =
// Address::from_str(&self.args[0]).unwrap().resolve(sim);                 //
// println!("{:?}", signature);                 let mut ent_model =
// EntityPrefabModel {                     name:
// StringId::from_truncate(&reg.name.to_string()),
// components: Vec::new(),                 };
//                 sim.model.entities.push(ent_model);
//
//                 // if do_spawn {
//                 //     sim.add_entity(
//                 //         &signature.get_ent_type_safe().unwrap(),
//                 //         &signature.get_ent_id_safe().unwrap(),
//                 //         &signature.get_ent_id_safe().unwrap(),
//                 //     );
//                 // }
//
//                 // CommandResult::Ok
//                 Ok(())
//             }
//             Register::Component(reg) => {
//                 trace!("executing register component cmd: {:?}", reg);
//
//                 let comp_model =
// sim.model.get_component(&reg.source_comp).unwrap();
// trace!("source_comp: {:?}", comp_model);
//
//                 let component = ComponentModel {
//                     name: reg.name.into(),
//                     start_state: StringId::from_unchecked("init"),
//                     triggers: reg.trigger_events.clone(),
//                     // logic: LogicModel {
//                     //     commands: comp_model.logic.commands.clone(),
//                     //     cmd_location_map:
// comp_model.logic.cmd_location_map.clone(),                     //
// ..LogicModel::default()                     // },
//                     logic: comp_model.logic.get_subset(reg.start_line,
// reg.end_line),                     ..ComponentModel::default()
//                 };
//
//                 debug!("registering component: {:?}", comp_model);
//                 sim.model.components.push(component);
//
//                 // let comp_model = ComponentModel {
//                 //     name: StringId::from_truncate(&reg.name.to_string()),
//                 //     vars: Vec::new(),
//                 //     start_state: StringId::from_unchecked("init"),
//                 //     triggers: reg.trigger_events.clone(),
//                 //     // triggers:
// vec![ShortString::from_str_truncate("step")],                 //     logic:
// crate::model::LogicModel::empty(),                 //     source_files:
// Vec::new(),                 //     script_files: Vec::new(),
//                 //     lib_files: Vec::new(),
//                 // };
//                 // debug!("registering component: {:?}", comp_model);
//                 // sim.model.components.push(comp_model);
//
//                 // if reg_comp.do_attach {
//                 //     for (&(ent_type, ent_id), mut entity) in &mut
// sim.entities {                 //         if &ent_type.as_str() ==
// &addr.get_ent_type_safe().unwrap().as_str() {                 //
// // entity.components.attach()                 //
// entity.components.attach(                 //                 &sim.model,
//                 //                 &mut entity.storage,
//                 //                 &addr.get_comp_type_safe().unwrap(),
//                 //                 &addr.get_comp_id_safe().unwrap(),
//                 //                 &addr.get_comp_id_safe().unwrap(),
//                 //             );
//                 //         }
//                 //     }
//                 // }
//
//                 Ok(())
//             }
//             Register::Event => Ok(()),
//             Register::Var(reg) => {
//                 debug!("registering var: {:?}", reg);
//
//                 sim.model
//                     .get_component_mut(&reg.comp)
//                     .unwrap()
//                     .vars
//                     .push(crate::model::VarModel {
//                         id: reg.addr.var_id.to_string(),
//                         type_: reg.addr.var_type,
//                         default: reg.val.clone(),
//                     });
//                 Ok(())
//
//                 //let mut comp_type_model = ComponentTypeModel {
//                 //id: signature.get_comp_type_safe().unwrap().to_string(),
//                 //entity_type:
// signature.get_ent_type_safe().unwrap().to_string(),                 //};
//                 //sim.model.component_types.push(comp_type_model);
//             }
//             Register::Trigger(reg) => {
//                 debug!("registering comp trigger: {:?}", reg);
//
//                 sim.model
//                     .get_component_mut(&reg.comp)
//                     .unwrap()
//                     .triggers
//                     .push(reg.name);
//                 Ok(())
//
//                 //let mut comp_type_model = ComponentTypeModel {
//                 //id: signature.get_comp_type_safe().unwrap().to_string(),
//                 //entity_type:
// signature.get_ent_type_safe().unwrap().to_string(),                 //};
//                 //sim.model.component_types.push(comp_type_model);
//             }
//             _ => Ok(()),
//         }
//     }
//
//     pub fn execute_ext_distr(
//         &self,
//         central: &mut SimCentral,
//         ent_name: &crate::EntityId,
//         comp_name: &crate::CompId,
//     ) -> Result<()> {
//         match &self {
//             Register::Entity(reg) => {
//                 debug!("registering entity prefab");
//                 let mut ent_model = EntityPrefabModel {
//                     name: StringId::from_truncate(&reg.name.to_string()),
//                     components: reg.components.clone(),
//                 };
//                 central.model.entities.push(ent_model);
//                 Ok(())
//             }
//             Register::Component(reg) => {
//                 debug!("registering component");
//                 let comp_model = ComponentModel {
//                     name: StringId::from_truncate(&reg.name.to_string()),
//                     vars: Vec::new(),
//                     start_state: StringId::from_unchecked("idle"),
//                     triggers: reg.trigger_events.clone(),
//                     // triggers:
// vec![ShortString::from_str_truncate("step")],                     logic:
// LogicModel::empty(),                     source_files: Vec::new(),
//                     script_files: Vec::new(),
//                     lib_files: Vec::new(),
//                 };
//                 // central.model_changes_queue.components.push(comp_model);
//                 central.model.components.push(comp_model);
//
//                 // if reg_comp.do_attach {
//                 //     for (&(ent_type, ent_id), mut entity) in &mut
// sim.entities {                 //         if &ent_type.as_str() ==
// &addr.get_ent_type_safe().unwrap().as_str() {                 //
// // entity.components.attach()                 //
// entity.components.attach(                 //                 &sim.model,
//                 //                 &mut entity.storage,
//                 //                 &addr.get_comp_type_safe().unwrap(),
//                 //                 &addr.get_comp_id_safe().unwrap(),
//                 //                 &addr.get_comp_id_safe().unwrap(),
//                 //             );
//                 //         }
//                 //     }
//                 // }
//
//                 Ok(())
//             }
//             Register::Event => Ok(()),
//             Register::Var(reg) => {
//                 debug!("registering var: {:?}", reg);
//
//                 central
//                     // .model_changes_queue
//                     .model
//                     .get_component_mut(&reg.comp)
//                     .unwrap()
//                     .vars
//                     .push(crate::model::VarModel {
//                         id: reg.addr.var_id.to_string(),
//                         type_: reg.addr.var_type,
//                         default: reg.val.clone(),
//                     });
//                 Ok(())
//
//                 //let mut comp_type_model = ComponentTypeModel {
//                 //id: signature.get_comp_type_safe().unwrap().to_string(),
//                 //entity_type:
// signature.get_ent_type_safe().unwrap().to_string(),                 //};
//                 //sim.model.component_types.push(comp_type_model);
//             }
//             Register::Trigger(reg) => {
//                 debug!("registering trigger: {:?}", reg);
//
//                 central
//                     .model
//                     .get_component_mut(&reg.comp)
//                     .unwrap()
//                     .triggers
//                     .push(reg.name);
//
//                 Ok(())
//             }
//             _ => Ok(()),
//         }
//     }
// }
