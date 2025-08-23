#[cfg(feature = "behavior_dynlib")]
pub mod dynlib;
#[cfg(feature = "behavior_lua")]
pub mod lua;

use futures::future::BoxFuture;
use futures::TryFutureExt;
use tokio::sync::oneshot;

use crate::executor::LocalExec;
use crate::model;
use crate::model::behavior::Behavior as BehaviorModel;
use crate::model::behavior::InstancingTarget;
use crate::rpc;
use crate::rpc::Signal;
use crate::worker::part::Partition;
use crate::CompName;
use crate::EntityName;
use crate::Error;
use crate::EventName;
use crate::Executor;
use crate::Model;
use crate::Result;

/// Possible targets for which a behavior can be spawned.
///
/// This enum is used for indexing behavior handles stored on the worker
/// partition. This arrangement is particularly useful in the case of entity
/// transfer between workers, as it allows for quickly rounding up behavior
/// instances bound to specific entities.
#[derive(Clone, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub enum BehaviorTarget {
    /// Behavior is bound to it's current worker partition.
    Worker,
    /// Behavior is bound to a particular entity.
    ///
    /// Note that this only points to the entity for which the behavior was
    /// spawned. More specific instancing rules are defined with the model.
    Entity(EntityName),
}

#[derive(Clone)]
pub struct BehaviorHandle {
    pub name: String,
    pub triggers: Vec<EventName>,
    pub executor:
        LocalExec<Signal<rpc::behavior::Request>, Result<Signal<rpc::behavior::Response>>>,
}

#[async_trait::async_trait]
impl Executor<Signal<rpc::behavior::Request>, Result<Signal<rpc::behavior::Response>>>
    for BehaviorHandle
{
    async fn execute(
        &self,
        req: Signal<rpc::behavior::Request>,
    ) -> crate::error::Result<Result<Signal<rpc::behavior::Response>>> {
        self.executor.execute(req).await
    }
}

pub fn spawn_non_entity_bound(
    model: &Model,
    part: &mut Partition,
    worker_exec: LocalExec<Signal<rpc::worker::Request>, Result<Signal<rpc::worker::Response>>>,
) -> Result<()> {
    for behavior in &model.behaviors {
        for instancing_rules in &behavior.targets {
            match instancing_rules {
                InstancingTarget::PerWorker => {
                    spawn_onto_partition(
                        behavior,
                        part,
                        BehaviorTarget::Worker,
                        part.behavior_exec.clone(),
                    )?;
                }
                // InstancingTarget::GlobalSingleton => {
                //     // NOTE: spawning of global singleton behaviors is always
                //     // initiated on the leader level.
                //     spawn_onto_partition(
                //         behavior,
                //         part,
                //         BehaviorTarget::Worker,
                //         part.behavior_exec.clone(),
                //     )?;
                // }
                _ => {
                    // NOTE: spawning of all entity-bound behaviors is handled
                    // with the entity spawning logic on the worker partition
                    // level.
                }
            }
        }
    }

    Ok(())
}

pub fn spawn_onto_partition(
    behavior: &BehaviorModel,
    part: &mut Partition,
    target: BehaviorTarget,
    worker_exec: LocalExec<Signal<rpc::worker::Request>, Result<Signal<rpc::worker::Response>>>,
) -> Result<()> {
    use model::behavior::BehaviorInner;
    match &behavior.inner {
        #[cfg(feature = "behavior_dynlib")]
        BehaviorInner::Dynlib {
            artifact,
            entry,
            synced,
            debug,
        } => {
            let (lib, handle) = dynlib::spawn(
                &behavior.name,
                artifact,
                entry,
                *synced,
                *debug,
                behavior.triggers.clone(),
                worker_exec.clone(),
                part.behavior_broadcast.0.clone(),
            )?;
            if let Some(mut handle) = handle {
                part.behaviors
                    .entry(target)
                    .and_modify(|handles| handles.push(handle.clone()))
                    .or_insert(vec![handle]);
            }
            part.libs.push(lib);
        }
        #[cfg(feature = "machine")]
        BehaviorInner::Machine { script, logic } => {
            let handle = crate::machine::spawn(
                Some(behavior.name.clone()),
                behavior.triggers.clone(),
                worker_exec.clone(),
            )?;
            part.machines.push(handle);
        }
        #[cfg(feature = "behavior_lua")]
        BehaviorInner::Lua { synced, script } => {
            if *synced {
                let handle = lua::spawn(
                    behavior.name.clone(),
                    script.to_owned(),
                    behavior.triggers.clone(),
                    target.clone(),
                    worker_exec.clone(),
                )?;
                part.behaviors
                    .entry(target)
                    .and_modify(|handles| handles.push(handle.clone()))
                    .or_insert(vec![handle]);
            } else {
                lua::spawn_unsynced(
                    script.to_owned(),
                    behavior.triggers.clone(),
                    part.behavior_broadcast.0.subscribe(),
                    worker_exec.clone(),
                )?;
            }
        }
        #[cfg(feature = "behavior_wasm")]
        BehaviorInner::Wasm {
            artifact,
            synced,
            debug,
        } => todo!(),
        _ => panic!("enable at least one `*_behavior` feature to be able to spawn behaviors"),
    }

    Ok(())
}

/// Spawns a new *unsynced* behavior task on the runtime.
///
/// *Unsynced* refers to the way requests are passed to the behavior task,
/// which is by a means of a broadcast queue. Behavior task is therefore not
/// able to send an acknowledgement response.
///
/// In the context of a clock-synchronized simulation, this means that the
/// unsynced behaviors process incoming requests on a best-effort basis, which
/// can lead to a slow-consumer problem. For example if the logic at some event
/// trigger takes too long, that event might already have been triggered again
/// before all logic from the original trigger is done executing.
pub fn spawn_unsynced(
    f: impl FnOnce(
        tokio::sync::broadcast::Receiver<rpc::behavior::Request>,
        LocalExec<Signal<rpc::worker::Request>, crate::Result<Signal<rpc::worker::Response>>>,
    ) -> BoxFuture<'static, Result<()>>,
    broadcast: tokio::sync::broadcast::Receiver<rpc::behavior::Request>,
    exec: LocalExec<Signal<rpc::worker::Request>, crate::Result<Signal<rpc::worker::Response>>>,
) -> Result<()> {
    tokio::spawn(
        f(broadcast, exec.clone())
            .inspect_err(|e| log::error!("behavior failed with: {}", e.to_string())),
    );

    Ok(())
}

/// Spawns a new *synced* behavior task on the runtime.
///
/// *Synced* here means that external communication with the behavior involves
/// a request-response executor.
///
/// Behavior handles are used to keep track of synced behaviors and to execute
/// potential requests that will always await subsequent responses. This way in
/// a clock-synchronized context the simulation execution can be postponed
/// until all synced behaviors "check-in" with their responses.
pub fn spawn_synced(
    f: impl FnOnce(
        tokio_stream::wrappers::ReceiverStream<(
            Signal<rpc::behavior::Request>,
            oneshot::Sender<Result<Signal<rpc::behavior::Response>>>,
        )>,
        LocalExec<Signal<rpc::worker::Request>, crate::Result<Signal<rpc::worker::Response>>>,
    ) -> BoxFuture<'static, Result<()>>,
    name: String,
    triggers: Vec<EventName>,
    exec: LocalExec<Signal<rpc::worker::Request>, crate::Result<Signal<rpc::worker::Response>>>,
) -> Result<BehaviorHandle> {
    let (executor, stream, stream_multi) = LocalExec::new(20);

    tokio::spawn(
        f(stream, exec.clone())
            .inspect_err(|e| log::error!("behavior failed with: {}", e.to_string())),
    );

    Ok(BehaviorHandle {
        name,
        triggers,
        executor,
    })
}

pub type BehaviorEventStreamSynced = tokio_stream::wrappers::ReceiverStream<(
    Signal<rpc::behavior::Request>,
    oneshot::Sender<Result<Signal<rpc::behavior::Response>>>,
)>;
pub type BehaviorEventStreamUnsynced = tokio::sync::broadcast::Receiver<rpc::behavior::Request>;

// TODO: should probably be defined in `worker::exec` module.
pub type BehaviorExec =
    LocalExec<Signal<rpc::worker::Request>, crate::Result<Signal<rpc::worker::Response>>>;

pub type BehaviorFnUnsynced =
    fn(BehaviorEventStreamUnsynced, BehaviorExec) -> BoxFuture<'static, Result<()>>;

pub type BehaviorFnSynced =
    fn(BehaviorEventStreamSynced, BehaviorExec) -> BoxFuture<'static, Result<()>>;
