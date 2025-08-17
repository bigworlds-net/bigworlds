//! Worker manager task holds the worker state and shares it via
//! message passing.

use std::fmt::{Display, Formatter};
use std::str::FromStr;
use std::time::{Duration, Instant};

use fnv::FnvHashMap;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::behavior::{BehaviorHandle, BehaviorTarget};
use crate::entity::Entity;
use crate::executor::{LocalExec, Signal};
use crate::net::CompositeAddress;
use crate::rpc::{Caller, Participant};
use crate::server::ServerId;
use crate::worker::part::Partition;
use crate::worker::{Leader, LeaderSituation, Server, ServerExec, State};
use crate::{
    net, query, rpc, util, Address, EntityName, Error, Executor, Model, PrefabName, Query,
    QueryProduct, Result, Var,
};

use super::{Config, OtherWorker, Subscription, WorkerId};

pub type ManagerExec = LocalExec<Request, Result<Response>>;

impl ManagerExec {
    pub async fn broadcast(&self) -> Result<()> {
        Ok(())
    }

    pub async fn get_id(&self) -> Result<WorkerId> {
        let resp = self.execute(Request::GetId).await??;
        if let Response::GetId(m) = resp {
            Ok(m)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_config(&self) -> Result<Config> {
        let resp = self.execute(Request::GetConfig).await??;
        if let Response::GetConfig(config) = resp {
            Ok(config)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn pull_config(&self, config: Config) -> Result<()> {
        let resp = self.execute(Request::PullConfig(config)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn process_query(&self, query: Query) -> Result<QueryProduct> {
        let resp = self.execute(Request::ProcessQuery(query)).await??;
        if let Response::ProcessQuery(product) = resp {
            Ok(product)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn subscribe(
        &self,
        triggers: Vec<query::Trigger>,
        query: Query,
        sender: mpsc::Sender<Result<Signal<rpc::worker::Response>>>,
    ) -> Result<Uuid> {
        let resp = self
            .execute(Request::Subscribe {
                triggers,
                query,
                sender,
            })
            .await??;
        if let Response::Subscribe(id) = resp {
            Ok(id)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn unsubscribe(&self, id: Uuid) -> Result<()> {
        let resp = self.execute(Request::Unsubscribe(id)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_leader(&self) -> Result<LeaderSituation> {
        let resp = self.execute(Request::GetLeader).await??;
        if let Response::GetLeader(leader) = resp {
            Ok(leader)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn set_leader(&self, leader: LeaderSituation) -> Result<()> {
        let resp = self.execute(Request::SetLeader(leader)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_other_workers(&self) -> Result<FnvHashMap<WorkerId, OtherWorker>> {
        let resp = self.execute(Request::GetOtherWorkers).await??;
        if let Response::GetOtherWorkers(workers) = resp {
            Ok(workers)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn add_other_worker(&self, worker: OtherWorker) -> Result<()> {
        let resp = self.execute(Request::AddOtherWorker(worker)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_listeners(&self) -> Result<Vec<CompositeAddress>> {
        let resp = self.execute(Request::GetListeners).await??;
        if let Response::GetListeners(listeners) = resp {
            Ok(listeners)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_model(&self) -> Result<Model> {
        let resp = self.execute(Request::GetModel).await??;
        if let Response::Model(model) = resp {
            Ok(model)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn set_model(&self, model: Model) -> Result<()> {
        let resp = self.execute(Request::SetModel(model)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_var(&self, address: Address) -> Result<Var> {
        Ok(self.execute(Request::GetVar(address)).await??.try_into()?)
    }

    pub async fn set_vars(&self, pairs: Vec<(Address, Var)>) -> Result<()> {
        Ok(drop(self.execute(Request::SetVars(pairs)).await??))
    }

    pub async fn memory_size(&self) -> Result<usize> {
        let resp = self.execute(Request::MemorySize).await??;
        if let Response::MemorySize(size) = resp {
            Ok(size)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_blocked_watch(&self) -> Result<watch::Receiver<bool>> {
        let resp = self.execute(Request::GetBlockedWatch).await??;
        if let Response::GetBlockedWatch(watch) = resp {
            Ok(watch)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn set_blocked_watch(&self, value: bool) -> Result<()> {
        let resp = self.execute(Request::SetBlockedWatch(value)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_clock_watch(&self) -> Result<watch::Receiver<usize>> {
        let resp = self.execute(Request::GetClockWatch).await??;
        if let Response::GetClockWatch(watch) = resp {
            Ok(watch)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn set_clock_watch(&self, value: usize) -> Result<()> {
        let resp = self.execute(Request::SetClockWatch(value)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_servers(&self) -> Result<FnvHashMap<ServerId, Server>> {
        let resp = self.execute(Request::GetServers).await??;
        if let Response::GetServers(servers) = resp {
            Ok(servers)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn insert_server(&self, server_id: ServerId, server: Server) -> Result<()> {
        let resp = self
            .execute(Request::InsertServer(server_id, server))
            .await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn initialize(
        &self,
        behavior_exec: LocalExec<
            Signal<rpc::worker::Request>,
            Result<Signal<rpc::worker::Response>>,
        >,
    ) -> Result<()> {
        let resp = self.execute(Request::Initialize(behavior_exec)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_synced_behavior_handles(
        &self,
    ) -> Result<FnvHashMap<BehaviorTarget, Vec<BehaviorHandle>>> {
        let resp = self.execute(Request::GetSyncedBehaviorHandles).await??;
        if let Response::GetSyncedBehaviorHandles(handles) = resp {
            Ok(handles)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_unsynced_behavior_tx(
        &self,
    ) -> Result<tokio::sync::broadcast::Sender<rpc::behavior::Request>> {
        let resp = self.execute(Request::GetUnsyncedBehaviorTx).await??;
        if let Response::GetUnsyncedBehaviorTx(tx) = resp {
            Ok(tx)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    #[cfg(feature = "machine")]
    pub async fn get_machine_handles(&self) -> Result<Vec<crate::machine::MachineHandle>> {
        let resp = self.execute(Request::GetMachineHandles).await??;
        if let Response::GetMachineHandles(handles) = resp {
            Ok(handles)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_entity(&self, name: EntityName) -> Result<Entity> {
        let resp = self.execute(Request::GetEntity(name)).await??;
        if let Response::Entity(entity) = resp {
            Ok(entity)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_entities(&self) -> Result<Vec<EntityName>> {
        let resp = self.execute(Request::GetEntities).await??;
        if let Response::Entities(entities) = resp {
            Ok(entities)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn spawn_entity(&self, name: EntityName, prefab: Option<PrefabName>) -> Result<()> {
        self.execute(Request::SpawnEntity(name, prefab)).await??;
        Ok(())
    }

    pub async fn remove_entity(&self, name: EntityName) -> Result<()> {
        self.execute(Request::RemoveEntity(name)).await??;
        Ok(())
    }

    pub async fn add_entity(&self, name: EntityName, entity: Entity) -> Result<()> {
        self.execute(Request::AddEntity(name, entity)).await??;
        Ok(())
    }

    pub async fn add_behavior(&self, target: BehaviorTarget, handle: BehaviorHandle) -> Result<()> {
        self.execute(Request::AddBehavior(target, handle)).await??;
        Ok(())
    }

    pub async fn get_subscriptions(&self) -> Result<Vec<Subscription>> {
        let resp = self.execute(Request::GetSubscriptions).await??;
        if let Response::Subscriptions(subs) = resp {
            Ok(subs)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.execute(Request::Shutdown).await??;
        Ok(())
    }
}

pub fn spawn(mut worker: State, cancel: CancellationToken) -> Result<ManagerExec> {
    use tokio_stream::StreamExt;

    let (exec, mut stream, _) = LocalExec::new(1);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some((req, snd)) = stream.next() => {
                    debug!("worker::manager: processing request: {req}");
                    let resp = handle_request(req, &mut worker).await;
                    snd.send(resp);
                },
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    if worker.config.partition.auto_archive_entities {
                        let now = Instant::now();
                        if let Some(ref mut part) = worker.part {
                            match part.archive_entities() {
                                Ok(count) => if count > 0 {
                                    debug!("archived {} entities, took: {}ms", count, now.elapsed().as_millis());
                                }
                                Err(e) => warn!("entity archival chore failed: {}", e),
                            }
                        }
                    }

                }
                _ = cancel.cancelled() => {
                    debug!("worker[{}]: manager: shutting down", worker.id);
                    handle_request(Request::Shutdown, &mut worker).await;
                    break;
                }
            }
        }
    });

    Ok(exec)
}

async fn handle_request(req: Request, mut worker: &mut State) -> Result<Response> {
    trace!("worker[{}]: manager: handling request: {req}", worker.id);
    match req {
        Request::GetId => Ok(Response::GetId(worker.id)),
        Request::GetConfig => Ok(Response::GetConfig(worker.config.clone())),
        Request::PullConfig(config) => {
            if let Some(part) = &mut worker.part {
                part.config = config.partition.clone();
            }
            worker.config = config;

            Ok(Response::Empty)
        }
        // TODO: consider a separate request for getting leader or err,
        // instead of returning the whole leader situation enum.
        Request::GetLeader => Ok(Response::GetLeader(worker.leader.clone())),
        Request::SetLeader(leader) => {
            worker.leader = leader;
            Ok(Response::Empty)
        }
        Request::InsertServer(server_id, server) => {
            worker.servers.insert(server_id, server);
            Ok(Response::Empty)
        }
        Request::GetBlockedWatch => Ok(Response::GetBlockedWatch(worker.blocked_watch.1.clone())),
        Request::SetBlockedWatch(value) => {
            worker.blocked_watch.0.send(value);
            Ok(Response::Empty)
        }
        Request::GetClockWatch => Ok(Response::GetClockWatch(worker.clock_watch.1.clone())),
        Request::SetClockWatch(value) => {
            trace!("setting clock watch to {}", value);
            worker.clock_watch.0.send(value);
            Ok(Response::Empty)
        }
        Request::GetServers => Ok(Response::GetServers(worker.servers.clone())),
        Request::GetEntity(name) => {
            if let Some(part) = &mut worker.part {
                if let Ok(entity) = part.get_entity(&name) {
                    Ok(Response::Entity(entity.to_owned()))
                } else {
                    Err(Error::FailedGettingEntityByName(name))
                }
            } else {
                Err(Error::WorkerNotInitialized(
                    "Partition not available".to_string(),
                ))
            }
        }
        Request::GetEntities => {
            if let Some(part) = &worker.part {
                let mut entities = part
                    .entities
                    .iter()
                    .map(|(k, _)| k.to_owned())
                    .collect::<Vec<EntityName>>();

                // TODO: include archived entities conditionally.
                entities.extend(
                    part.archive
                        .iter()
                        .map(|k| EntityName::from_utf8(k.unwrap().0.to_vec()).unwrap()),
                );

                Ok(Response::Entities(entities))
            } else {
                Err(Error::WorkerNotInitialized(
                    "Partition not available".to_string(),
                ))
            }
        }
        Request::GetSyncedBehaviorHandles => {
            if let Some(part) = &worker.part {
                Ok(Response::GetSyncedBehaviorHandles(part.behaviors.clone()))
            } else {
                Err(Error::WorkerNotInitialized(
                    "Partition not available".to_string(),
                ))
            }
        }
        Request::GetUnsyncedBehaviorTx => {
            if let Some(part) = &worker.part {
                Ok(Response::GetUnsyncedBehaviorTx(
                    part.behavior_broadcast.0.clone(),
                ))
            } else {
                Err(Error::WorkerNotInitialized(
                    "Partition not available".to_string(),
                ))
            }
        }
        #[cfg(feature = "machine")]
        Request::GetMachineHandles => {
            if let Some(part) = &worker.part {
                Ok(Response::GetMachineHandles(part.machines.clone()))
            } else {
                Err(Error::WorkerNotInitialized(
                    "Partition not available".to_string(),
                ))
            }
        }
        Request::MemorySize => {
            if let Some(part) = &worker.part {
                unimplemented!();
            } else {
                Err(Error::LeaderNotInitialized(format!(
                    "Partition not available"
                )))
            }
        }
        Request::GetClock => {
            unimplemented!();
            // snd.send(Response::Clock(worker.clock));
        }
        Request::ProcessQuery(query) => {
            if let Some(part) = &mut worker.part {
                // let now = std::time::Instant::now();
                let product = crate::query::process_query(&query, part).await?;
                // println!("processed query: elapsed: {}ms", now.elapsed().as_millis());
                Ok(Response::ProcessQuery(product))
            } else {
                Err(Error::WorkerNotInitialized(format!(
                    "node not available, was simulation model loaded onto the cluster?"
                )))
            }
        }
        Request::Subscribe {
            triggers,
            query,
            sender,
        } => {
            let id = Uuid::new_v4();
            worker.subscriptions.push(super::Subscription {
                id,
                triggers,
                query,
                sender,
            });
            Ok(Response::Subscribe(id))
        }
        Request::Unsubscribe(id) => {
            // if let Some(sub) = worker.subscriptions.iter_mut().find(|sub| sub.id == id) {
            // sub.sender.is_closed()
            // drop(sub.sender);
            // }
            worker.subscriptions.retain(|sub| sub.id != id);
            Ok(Response::Empty)
        }
        Request::GetSubscriptions => Ok(Response::Subscriptions(worker.subscriptions.clone())),

        Request::GetModel => {
            if let Some(model) = &worker.model {
                Ok(Response::Model(model.clone()))
            } else {
                Err(Error::WorkerNotInitialized(format!("model is not set")))
            }
        }
        Request::SetModel(model) => {
            worker.model = Some(model);
            Ok(Response::Empty)
        }
        Request::Initialize(behavior_exec) => {
            if let Some(model) = &worker.model {
                // HACK: we use replace semantics since the broadcast channel
                // is stored on the partition. It's still not clear, but
                // perhaps the channel should live higher up on the worker
                // state instead.
                let old_part = worker.part.replace(
                    Partition::from_model(
                        model,
                        behavior_exec,
                        worker.config.partition.clone(),
                        worker.id,
                    )
                    .await?,
                );
                if let Some(old_part) = old_part {
                    worker.part.as_mut().unwrap().behavior_broadcast = old_part.behavior_broadcast;
                }

                Ok(Response::Empty)
            } else {
                Err(Error::WorkerNotInitialized(format!("model is not set")))
            }
        }
        Request::SpawnMachine => {
            unimplemented!();
            // let handle = machine::spawn(
            // worker.machine_pipe.clone(),
            // tokio::runtime::Handle::current(),
            // )?;
            // if let Some(part) = &mut worker.part {
            // part.machines.insert("_machine_ent".to_string(), vec![]);
            // }
            // Ok(Response::MachineHandle(handle))
        }
        Request::Shutdown => {
            if let Some(part) = &worker.part {
                log::trace!("manager sending shutdown to behavior tasks");
                part.behavior_broadcast
                    .0
                    .send(rpc::behavior::Request::Shutdown)
                    .inspect_err(|e| {
                        trace!("behavior broadcast error: shutdown: no receivers: {e}")
                    });
                for (target, handles) in &part.behaviors {
                    for handle in handles {
                        handle
                            .execute(Signal::from(rpc::behavior::Request::Shutdown))
                            .await
                            .inspect_err(|e| {
                                println!("behavior handle execute error: shutdown: {e}")
                            });
                    }
                }

                // Let the leader know we're shutting down.
                if let LeaderSituation::Connected(leader) = &worker.leader {
                    // println!(">>> letting leader know we're shutting down the worker");
                    leader
                        .execute(
                            Signal::from(rpc::leader::Request::DisconnectWorker)
                                .originating_at(Participant::Worker(worker.id).into()),
                        )
                        .await?;
                }

                Ok(Response::Empty)
            } else {
                Err(Error::WorkerNotInitialized(format!(
                    "can't shutdown items on worker part"
                )))
            }
        }
        Request::AddEntity(name, entity) => {
            if let Some(model) = &worker.model {
                if let Some(ref mut part) = worker.part {
                    part.add_entity(name, entity, model)?;
                    Ok(Response::Empty)
                } else {
                    Err(Error::WorkerNotInitialized(format!(
                        "Worker part unavailable"
                    )))
                }
            } else {
                Err(Error::WorkerNotInitialized(format!(
                    "Worker model unavailable"
                )))
            }
        }
        Request::SpawnEntity(name, prefab) => {
            if let Some(model) = &worker.model {
                if let Some(ref mut part) = worker.part {
                    part.spawn_entity(name, &prefab, &Default::default(), model)?;
                    Ok(Response::Empty)
                } else {
                    Err(Error::WorkerNotInitialized(format!(
                        "Worker part unavailable"
                    )))
                }
            } else {
                Err(Error::WorkerNotInitialized(format!(
                    "Worker model unavailable"
                )))
            }
        }
        Request::RemoveEntity(name) => {
            if let Some(part) = &mut worker.part {
                part.remove_entity(name.clone())?;
                Ok(Response::Empty)
            } else {
                Err(Error::WorkerNotInitialized(format!(
                    "Worker part unavailable"
                )))
            }
        }
        Request::GetOtherWorkers => Ok(Response::GetOtherWorkers(worker.other_workers.clone())),
        Request::AddOtherWorker(other_worker) => {
            worker.other_workers.insert(other_worker.id, other_worker);
            Ok(Response::Empty)
        }
        Request::GetListeners => Ok(Response::GetListeners(worker.listeners.clone())),
        Request::SetVars(pairs) => {
            // TODO: expand to possibly set data on remote worker.
            for (address, var) in pairs {
                if let Some(part) = &mut worker.part {
                    if let Some(entity) = part.entities.get_mut(&address.entity) {
                        entity
                            .storage
                            .set_from_var(&address.to_local(), var.clone())?
                    }
                    // if let Ok(mut entity) = part.get_entity(&address.entity) {
                    //     let entity_id = address.entity.clone();
                    //     entity
                    //         .storage
                    //         .set_from_var(&address.to_local(), var.clone())?;
                    //     part.archive
                    //         .insert(
                    //             entity_id,
                    //             rkyv::to_bytes::<rkyv::rancor::Error>(entity)
                    //                 .unwrap()
                    //                 .as_slice(),
                    //         )
                    //         .unwrap();
                    // }
                }
            }
            Ok(Response::Empty)
        }
        Request::GetVar(address) => {
            if let Some(part) = &mut worker.part {
                if let Ok(entity) = part.get_entity(&address.entity) {
                    Ok(Response::GetVar(
                        entity.storage.get_var(&address.storage_index())?.clone(),
                    ))
                } else {
                    Err(Error::FailedGettingVar(address))
                }
            } else {
                Err(Error::WorkerNotInitialized(format!("part not available")))
            }
        }
        Request::AddBehavior(target, handle) => {
            if let Some(part) = &mut worker.part {
                part.behaviors
                    .entry(target)
                    // TODO: can we omit cloning here while still using the
                    // entry interface?
                    .and_modify(|handles| handles.push(handle.clone()))
                    .or_insert(vec![handle]);
                Ok(Response::Empty)
            } else {
                Err(Error::WorkerNotInitialized(format!("part not available")))
            }
        }
    }
}

#[derive(Clone, strum::Display)]
pub enum Request {
    GetId,
    GetConfig,
    PullConfig(Config),
    GetClock,
    GetBlockedWatch,
    SetBlockedWatch(bool),
    GetClockWatch,
    SetClockWatch(usize),
    GetServers,
    GetOtherWorkers,
    AddOtherWorker(OtherWorker),
    GetEntities,
    GetSyncedBehaviorHandles,
    GetUnsyncedBehaviorTx,
    MemorySize,
    GetLeader,
    SetLeader(LeaderSituation),
    InsertServer(ServerId, Server),

    ProcessQuery(Query),

    Subscribe {
        triggers: Vec<query::Trigger>,
        query: Query,
        sender: mpsc::Sender<Result<Signal<rpc::worker::Response>>>,
    },
    Unsubscribe(Uuid),
    GetSubscriptions,

    GetModel,
    SetModel(Model),
    SetVars(Vec<(Address, Var)>),
    GetVar(Address),
    Initialize(LocalExec<Signal<rpc::worker::Request>, Result<Signal<rpc::worker::Response>>>),
    SpawnMachine,
    Shutdown,
    SpawnEntity(EntityName, Option<PrefabName>),
    AddEntity(EntityName, Entity),
    RemoveEntity(EntityName),
    GetEntity(EntityName),
    GetListeners,
    AddBehavior(BehaviorTarget, BehaviorHandle),

    #[cfg(feature = "machine")]
    GetMachineHandles,
}

#[derive(Clone, strum::Display)]
pub enum Response {
    GetId(WorkerId),
    GetConfig(super::Config),
    GetBlockedWatch(watch::Receiver<bool>),
    GetClockWatch(watch::Receiver<usize>),
    GetServers(FnvHashMap<ServerId, Server>),
    GetOtherWorkers(FnvHashMap<WorkerId, OtherWorker>),
    GetLeader(LeaderSituation),
    GetSyncedBehaviorHandles(FnvHashMap<BehaviorTarget, Vec<BehaviorHandle>>),
    GetUnsyncedBehaviorTx(tokio::sync::broadcast::Sender<rpc::behavior::Request>),
    GetListeners(Vec<CompositeAddress>),
    GetVar(Var),
    ProcessQuery(QueryProduct),
    Subscribe(Uuid),
    Entities(Vec<EntityName>),
    Entity(Entity),
    Model(Model),
    MemorySize(usize),

    Subscriptions(Vec<Subscription>),

    Empty,

    #[cfg(feature = "machine")]
    GetMachineHandles(Vec<crate::machine::MachineHandle>),
}

impl TryInto<Var> for Response {
    type Error = Error;

    fn try_into(self) -> std::result::Result<Var, Self::Error> {
        match self {
            Response::GetVar(var) => Ok(var),
            _ => Err(Error::UnexpectedResponse(self.to_string())),
        }
    }
}
