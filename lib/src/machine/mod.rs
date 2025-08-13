//! `machine` denotes a [`behavior`] task set up to act as a virtual machine
//! executing a proprietary instruction set.
//!
//! The name can also be taken as hinting at the *(finite) state machine*
//! functionality the `machine` construct provides.
//!
//! # Instancing
//!
//! `machine` is provided alongside an instancing mechanism, the same that's
//! used for [`behavior`]. It allows for automatically spawning `machine`s
//! based on preselected requirements. For example we could spawn the same
//! piece of logic for each entity that has some specific component attached.
//!
//! # Security
//!
//! Because `machine` is only able to process a predefined set of instructions
//! it's great for sandboxing.
//!
//! In contrast, dynamic library behaviors can execute arbitrary code, which
//! can be dangerous when running untrusted models.
//!
//!
//! [`behavior`]: crate::behavior

pub mod cmd;
pub mod error;
pub mod exec;
pub mod script;

pub use cmd::Command;
use cmd::CommandResult;
pub use error::{Error, ErrorKind, Result};
use fnv::FnvHashMap;

use std::collections::BTreeMap;

use arrayvec::ArrayVec;
use smallvec::SmallVec;
use tokio_stream::StreamExt;

use crate::address::LocalAddress;
use crate::behavior::BehaviorHandle;
use crate::entity::StorageIndex;
use crate::executor::{LocalExec, Signal};
use crate::model::Var;
use crate::query::{Query, QueryProduct};
use crate::Address;
use crate::{rpc, CompName, EntityId, EntityName, EventName, Executor, VarType};

pub const START_STATE_NAME: &'static str = "start";

/// Internal runtime representation of a `machine` behavior.
///
/// Note it's mostly a wrapper over an executor exposing some additional
/// functionality for running requests on the worker.
#[derive(Clone)]
pub struct Machine {
    behavior: Option<String>,
    /// Current state.
    state: String,
    worker: LocalExec<Signal<rpc::worker::Request>, crate::Result<Signal<rpc::worker::Response>>>,
}

impl Machine {
    /// Triggers selected events.
    ///
    /// By default the request will get propagated across the cluster,
    /// reaching all available workers.
    pub async fn invoke(&self, events: Vec<EventName>) -> Result<()> {
        let response = self
            .worker
            .execute(Signal::from(rpc::worker::Request::Trigger(events)))
            .await??;
        Ok(())
    }

    pub async fn set_var(&self, addr: Address, var: crate::Var) -> Result<()> {
        let response = self
            .worker
            .execute(Signal::from(rpc::worker::Request::SetVar(addr, var)))
            .await??;

        if let rpc::worker::Response::Empty = response.into_payload() {
            Ok(())
        } else {
            unimplemented!()
        }
    }

    pub async fn get_var(&self, addr: Address) -> Result<crate::Var> {
        let response = self
            .worker
            .execute(Signal::from(rpc::worker::Request::GetVar(addr)))
            .await??;

        if let rpc::worker::Response::GetVar(var) = response.into_payload() {
            Ok(var)
        } else {
            unimplemented!()
        }
    }

    /// Retrieves a value at a given address and coerces it to string.
    pub async fn get_as_string(&self, addr: Address) -> Result<String> {
        let var = self.get_var(addr).await?;
        Ok(var.to_string())
    }

    /// Queries the worker using the standard query interface.
    pub async fn query(&self, query: Query) -> Result<QueryProduct> {
        let response = self
            .worker
            .execute(Signal::from(rpc::worker::Request::ProcessQuery(query)))
            .await??;

        if let rpc::worker::Response::Query(product) = response.into_payload() {
            Ok(product)
        } else {
            unimplemented!()
        }
    }
}

/// Handle to a live machine task.
#[derive(Clone)]
pub struct MachineHandle {
    executor: LocalExec<rpc::machine::Request, crate::Result<rpc::machine::Response>>,
    pub behavior: BehaviorHandle,
}

#[async_trait::async_trait]
impl Executor<rpc::machine::Request, crate::Result<rpc::machine::Response>> for MachineHandle {
    async fn execute(
        &self,
        req: rpc::machine::Request,
    ) -> crate::Result<crate::Result<rpc::machine::Response>> {
        self.executor.execute(req).await
    }
}

impl MachineHandle {
    pub async fn execute_cmd(&self, cmd: cmd::Command) -> crate::Result<rpc::machine::Response> {
        self.execute(rpc::machine::Request::Execute(cmd)).await?
    }

    pub async fn execute_cmd_batch(
        &self,
        cmds: Vec<cmd::Command>,
    ) -> crate::Result<rpc::machine::Response> {
        self.execute(rpc::machine::Request::ExecuteBatch(cmds))
            .await?
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.behavior
            .execute(Signal::from(rpc::behavior::Request::Shutdown))
            .await
            .map_err(|e| Error::new(LocationInfo::empty(), ErrorKind::CoreError(e.to_string())))
            .map(|_| ())
    }
}

/// Spawns a new machine task.
///
/// Returns a handle that can be used to communicate with the machine from the
/// outside.
///
/// The handle supports operations specific to machine operations as opposed to
/// regular behaviors. Internally the machine task uses the generic behavior
/// executor with it's standard request/response types.
///
// TODO: pass in context containing optional information about the local entity
// and component. This way in the scripts we can use local addresses valid for
// all machine instances, e.g. `int.hello` will be expanded to
// `entity1.component1.int.hello` when executing individual commands.
pub fn spawn(
    behavior_name: Option<String>,
    triggers: Vec<EventName>,
    worker: LocalExec<Signal<rpc::worker::Request>, crate::Result<Signal<rpc::worker::Response>>>,
) -> Result<MachineHandle> {
    let (executor, mut stream, _) = LocalExec::new(20);

    let behavior_name_ = behavior_name.clone();
    let f = |mut behavior_stream: tokio_stream::wrappers::ReceiverStream<(
        Signal<rpc::behavior::Request>,
        tokio::sync::oneshot::Sender<crate::Result<Signal<rpc::behavior::Response>>>,
    )>,
             worker| {
        Box::pin(async move {
            let mut machine = Machine {
                behavior: behavior_name_,
                state: "start".to_owned(),
                worker,
            };

            loop {
                tokio::select! {
                    Some((request, send)) = stream.next() => {
                        match request {
                            rpc::machine::Request::Step => {
                                if let Some(behavior_script) = &machine.behavior {
                                    let resp = machine.worker.execute(Signal::from(rpc::worker::Request::MachineLogic { name: behavior_script.clone() })).await??.into_payload();
                                    if let rpc::worker::Response::MachineLogic(logic) = resp {
                                        // No states declared, default to
                                        // executing the whole thing.
                                        if logic.states.len() == 1 {
                                            machine.state = "_main".to_owned();
                                        }
                                        exec::execute(logic, &mut machine).await?;
                                    } else {
                                        unimplemented!()
                                    }
                                }
                                send.send(Ok(rpc::machine::Response::Empty));
                            },
                            rpc::machine::Request::ExecuteBatch(cmds) => {
                                println!("executing cmds (batch): {cmds:?}");

                                let mut logic = Logic::default();
                                let mut registry = Registry {
                                    str0: String::new(),
                                    int0: 0,
                                    float0: 0.,
                                    bool0: false,
                                };

                                for cmd in cmds {
                                    let result = cmd
                                        .execute(
                                            &mut machine,
                                            &mut logic,
                                            &mut ArrayVec::new(),
                                            &mut registry,
                                            &LocationInfo::empty(),
                                        )
                                        .await;
                                }

                                send.send(Ok(rpc::machine::Response::Empty));
                            }
                            rpc::machine::Request::Execute(cmd) => {
                                let mut logic = if let Some(behavior_name) = &machine.behavior {
                                    let resp = machine.worker.execute(Signal::from(rpc::worker::Request::MachineLogic { name: behavior_name.clone() })).await??.into_payload();
                                    if let rpc::worker::Response::MachineLogic(logic) = resp {
                                        logic
                                    } else {
                                        panic!("unexpected response: {:?}", resp);
                                    }
                                } else {
                                    Logic::default()
                                };

                            let result = cmd
                                .execute(
                                    &mut machine,
                                    &mut logic,
                                    &mut ArrayVec::new(),
                                    &mut Registry {
                                        str0: String::new(),
                                        int0: 0,
                                        float0: 0.,
                                        bool0: false,
                                    },
                                    &LocationInfo::empty(),
                                )
                                .await;

                                // Include potential error in the response.
                                if let CommandResult::Err(e) = result {
                                    send.send(Err(crate::Error::MachinePanic(e)));
                                } else {
                                    send.send(Ok(rpc::machine::Response::Empty));
                                }

                            }
                            _ => unimplemented!(),
                        }
                    }
                    Some((request, send)) = behavior_stream.next() => {
                        match request {
                            Signal{ payload: rpc::behavior::Request::Event(event), ctx, .. } => {
                                // println!("machine got event: {}", event);
                                if let Some(behavior_script) = &machine.behavior {
                                    let resp = machine.worker.execute(Signal::from(rpc::worker::Request::MachineLogic { name: behavior_script.clone() })).await??.into_payload();
                                    if let rpc::worker::Response::MachineLogic(logic) = resp {
                                        exec::execute(logic, &mut machine).await?;
                                    } else {
                                        unimplemented!()
                                    }
                                }
                                send.send(Ok(Signal::new(rpc::behavior::Response::Empty, ctx)));
                            }
                            Signal{ payload: rpc::behavior::Request::Shutdown, ctx, .. } =>  {
                                send.send(Ok(Signal::new(rpc::behavior::Response::Empty, ctx)));
                                return Ok(());
                            }
                            _ => unimplemented!(),
                        }
                    }
                    else => {
                        continue;
                    }

                }
            }
            Ok(())
        }) as _
    };

    let behavior =
        crate::behavior::spawn_synced(f, behavior_name.unwrap_or("".to_owned()), triggers, worker)?;

    Ok(MachineHandle { executor, behavior })
}

/// Logic for a single self-contained machine.
#[cfg(feature = "machine")]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Logic {
    /// Name of the starting state
    pub start_state: String,
    /// List of commands
    pub commands: Vec<crate::machine::cmd::Command>,
    /// Mapping of state procedure names to their start and end lines
    pub states: FnvHashMap<String, (usize, usize)>,
    /// Mapping of non-state procedure names to their start and end lines
    pub procedures: FnvHashMap<String, (usize, usize)>,
    /// Location info mapped for each command on the list by index
    pub cmd_location_map: Vec<crate::machine::LocationInfo>,
}

impl Default for Logic {
    fn default() -> Self {
        Logic {
            start_state: START_STATE_NAME.to_owned(),
            commands: Vec::new(),
            states: FnvHashMap::default(),
            procedures: FnvHashMap::default(),
            cmd_location_map: Vec::new(),
        }
    }
}

impl Logic {
    pub fn from_script(text: &str, path: Option<String>) -> crate::Result<Self> {
        let mut instructions = script::parse_script(text, LocationInfo::empty()).unwrap();

        let mut logic = Logic::default();

        // Use script processor to handle scripts
        let program_data = crate::machine::script::util::get_program_metadata();

        // Preprocess entry script
        script::preprocessor::run(
            &mut instructions,
            // HACK
            &mut crate::Model::default(),
            // HACK
            &Default::default(),
        )
        .unwrap();

        // Turn instructions into proper commands.

        let mut commands: Vec<Command> = Vec::new();
        let mut cmd_prototypes: Vec<CommandPrototype> = Vec::new();
        let mut cmd_locations: Vec<LocationInfo> = Vec::new();

        // First get a list of commands from the main instruction list.
        for instruction in instructions {
            if let script::InstructionType::Command(proto) = instruction.type_ {
                cmd_prototypes.push(proto);
                cmd_locations.push(instruction.location.clone());
            }
        }

        for (n, cmd_prototype) in cmd_prototypes.iter().enumerate() {
            // cmd_locations[n].behavior_name = Some(comp_model.name.clone());
            cmd_locations[n].line = Some(n);
            if let Some(path) = &path {
                cmd_locations[n].source = Some(path.clone());
            }

            // create command struct from prototype
            let command =
                Command::from_prototype(cmd_prototype, &cmd_locations[n], &cmd_prototypes)?;
            commands.push(command.clone());

            // insert the commands into the component's logic model
            if let Command::Procedure(proc) = &command {
                logic
                    .procedures
                    .insert(proc.name.clone(), (proc.start_line, proc.end_line));
            }
            if let Command::State(state) = &command {
                logic
                    .states
                    .insert(state.name.clone(), (state.start_line, state.end_line));
            }

            logic.commands.push(command);
            logic
                .cmd_location_map
                //.insert(comp_model.logic.commands.len() - 1, location.clone());
                .insert(n, cmd_locations[n].clone());
        }

        logic.states.insert("_main".to_owned(), (0, commands.len()));

        // println!("{:?}", logic);

        Ok(logic)
    }
}

#[cfg(feature = "machine")]
impl Logic {
    pub fn get_subset(&self, start_line: usize, last_line: usize) -> Logic {
        let mut new_logic = Logic::default();
        new_logic.commands = self.commands[start_line..last_line].to_vec();
        new_logic.cmd_location_map = self.cmd_location_map[start_line..last_line].to_vec();
        // warn!("{:?}", new_logic);
        new_logic
    }

    // pub fn from_deser(entry: deser::C) -> Result<VarModel> {}
}

/// Holds instruction location information.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
// #[cfg_attr(feature = "small_stringid", derive(Copy))]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct LocationInfo {
    /// Path to project root
    pub root: Option<String>,
    /// Path to the source file, relative to project root
    pub source: Option<String>,
    /// Line number as seen in source file
    pub source_line: Option<usize>,
    /// Line number after trimming empty lines, aka command index
    pub line: Option<usize>,
    /// Unique tag for this location
    pub tag: Option<String>,
    // pub behavior_name: Option<StringId>,
}

impl LocationInfo {
    pub fn to_string(&self) -> String {
        format!(
            "Source: {}, Line: {}",
            self.source.as_ref().unwrap_or(&String::from("unknown")),
            self.source_line.as_ref().unwrap_or(&0)
        )
    }
    pub fn empty() -> LocationInfo {
        LocationInfo {
            root: None,
            source: None,
            source_line: None,
            line: None,
            tag: None,
        }
    }

    pub fn with_source(mut self, root: &str, source: &str) -> Self {
        self.root = Some(root.parse().unwrap());
        self.source = Some(source.parse().unwrap());
        self
    }
}

/// Command in it's simplest form, ready to be turned into a more concrete
/// representation.
///
/// Example:
/// ```text
/// command_name --flag --arg="something" -> output:address
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct CommandPrototype {
    /// Command name
    pub name: Option<String>,
    /// Command arguments
    pub arguments: Option<Vec<String>>,
    /// Command output
    pub output: Option<String>,
}

/// Custom collection type used as the main call stack during logic execution.
//TODO determine optimal size, determine whether it should be fixed size or not
pub(crate) type CallStackVec = ArrayVec<CallInfo, 32>;

/// Collection type used to hold command results.
pub(crate) type CommandResultVec = SmallVec<[cmd::CommandResult; 2]>;

/// Struct containing basic information about where the execution is
/// taking place.
#[derive(Debug, Clone, Serialize, Deserialize)]
// #[cfg_attr(feature = "small_stringid", derive(Copy))]
pub struct ExecutionContext {
    pub ent: EntityId,
    pub comp: CompName,
    pub location: LocationInfo,
}

/// List of "stack" variables available only to the component machine
/// and not visible from the outside.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub str0: String,
    pub int0: i32,
    pub float0: f32,
    pub bool0: bool,
}
impl Registry {
    pub fn new() -> Registry {
        Registry {
            str0: String::new(),
            int0: 0,
            float0: 0.0,
            bool0: false,
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RegistryTarget {
    Str0,
    Int0,
    Float0,
    Bool0,
}

/// Information about a single call.
#[derive(Debug, Clone, Serialize, Deserialize)]
// #[cfg_attr(feature = "small_stringid", derive(Copy))]
pub enum CallInfo {
    Procedure(ProcedureCallInfo),
    ForIn(ForInCallInfo),
    Loop(LoopCallInfo),
    IfElse(IfElseCallInfo),

    Component(ComponentCallInfo),
}

/// Information about a single procedure call.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ProcedureCallInfo {
    pub call_line: usize,
    pub start_line: usize,
    pub end_line: usize,
    // pub output_variable: Option<String>,
}

/// Information about a single forin call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
pub struct ForInCallInfo {
    /// Target that will be iterated over
    pub target: Option<LocalAddress>,
    pub target_len: usize,
    /// Variable to update while iterating
    pub variable: Option<LocalAddress>,
    // pub variable_type: Option<VarType>,
    /// Current iteration
    pub iteration: usize,
    // meta
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LoopCallInfo {
    start: usize,
    end: usize,
}

/// Contains information about a single ifelse call.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IfElseCallInfo {
    pub current: usize,
    pub passed: bool,
    pub else_line_index: usize,
    pub meta: IfElseMetaData,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IfElseMetaData {
    pub start: usize,
    pub end: usize,
    pub else_lines: [usize; 10],
}

/// Contains information about a single component block call.
#[derive(Debug, Clone, Serialize, Deserialize)]
// #[cfg_attr(feature = "small_stringid", derive(Copy))]
pub struct ComponentCallInfo {
    pub name: CompName,
    pub vars: Vec<Var>,

    pub start_line: usize,
    pub end_line: usize,
    // pub current: usize,
    // pub passed: bool,
    // pub else_line_index: usize,
    // pub meta: IfElseMetaData,
}

/// Performs a command search on the provided command prototype list.
///
/// Goal is to find the end, and potentially intermediate parts, of a block.
/// To accomplish this, the function takes lists of defs describing beginning,
/// middle and ending marks of any blocks that it may stumble upon during
/// the search.
///
/// On success returns a tuple of single end part line numer and list of middle
/// part line numbers. If no matching parts are found, and no error .
pub(crate) fn command_search(
    location: &LocationInfo,
    commands: &Vec<CommandPrototype>,
    constraints: (usize, Option<usize>),
    defs: (&Vec<&str>, &Vec<&str>, &Vec<&str>),
    blocks: (&Vec<&str>, &Vec<&str>),
    recurse: bool,
) -> Result<Option<(usize, Vec<usize>)>> {
    if defs.0.is_empty() {
        return Err(Error::new(
            location.clone(),
            ErrorKind::CommandSearchFailed(
                "command search requires begin definitions to be non-empty".to_string(),
            ),
        ));
    }
    if defs.2.is_empty() {
        return Err(Error::new(
            location.clone(),
            ErrorKind::CommandSearchFailed(
                "command search requires ending definitions to be non-empty".to_string(),
            ),
        ));
    }
    let mut locs = (0, Vec::new());
    let mut skip_to = constraints.0;
    let mut block_diff = 0;
    let finish_idx = commands.len();
    for line in constraints.0..finish_idx {
        if line >= skip_to {
            let command = &commands[line];
            match &command.name {
                Some(command) => {
                    if blocks.0.contains(&command.as_str()) {
                        block_diff = block_diff + 1;
                    } else if defs.1.contains(&command.as_str()) {
                        locs.1.push(line);
                    } else if blocks.1.contains(&command.as_str()) && block_diff > 0 {
                        block_diff = block_diff - 1;
                    } else if defs.2.contains(&command.as_str()) {
                        locs.0 = line;
                        return Ok(Some(locs));
                    } else if defs.0.contains(&command.as_str()) {
                        if recurse {
                            match command_search(
                                location,
                                commands,
                                (line + 1, Some(finish_idx)),
                                defs,
                                blocks,
                                recurse,
                            ) {
                                Ok(locs_opt) => match locs_opt {
                                    Some(_locs) => {
                                        skip_to = _locs.0 + 1;
                                        ()
                                    }
                                    None => {
                                        return Err(Error::new(
                                            location.clone(),
                                            ErrorKind::CommandSearchFailed(format!(
                                                "bad nesting: got {} but end not found",
                                                command
                                            )),
                                        ))
                                    }
                                },
                                Err(error) => return Err(error),
                            };
                        } else {
                            return Err(Error::new(
                                location.clone(),
                                ErrorKind::CommandSearchFailed(format!(
                                    "bad nesting: got {}",
                                    command,
                                )),
                            ));
                        }
                    }
                    ()
                }
                None => (),
            }
        }
    }

    Err(Error::new(
        location.clone(),
        ErrorKind::CommandSearchFailed(format!(
            "no end of structure for begin defs: {:?}",
            &defs.0
        )),
    ))
}
