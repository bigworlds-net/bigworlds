use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use fnv::FnvHashMap;
use futures::future::BoxFuture;
use itertools::Itertools;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::behavior::BehaviorTarget;
use crate::client::{self, AsyncClient};
use crate::entity::Entity;
use crate::executor::{Executor, ExecutorMulti, LocalExec, Signal};
use crate::net::CompositeAddress;
use crate::rpc::msg::{
    self, AdvanceRequest, Message, RegisterClientRequest, RegisterClientResponse,
};
use crate::rpc::worker::RequestLocal;
use crate::snapshot::Snapshot;
use crate::{behavior, query, Query};
use crate::{
    leader, rpc, server, worker, Address, EntityId, EntityName, Error, Model, Result, Var,
};
use crate::{EventName, PrefabName};

#[cfg(feature = "machine")]
use crate::machine::MachineHandle;

#[derive(Clone)]
pub struct SimConfig {
    /// Number of workers to be spawned as part of the local sim instance.
    /// Defaults to the number of available (logical) CPU cores.
    pub worker_count: usize,

    /// Worker configuration is the same for all the local workers.
    pub worker: worker::Config,

    pub leader: leader::Config,

    /// Server configuration is optional as we can have a valid `Sim` instance
    /// without a server task.
    pub server: Option<server::Config>,

    /// Flag controlling the optional automatic step processing.
    pub autostep: Option<Duration>,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            worker_count: num_cpus::get(),
            worker: worker::Config::default(),
            leader: leader::Config::default(),
            server: None,
            autostep: None,
        }
    }
}

/// Spawns a local simulation instance.
///
/// This is a convenient method for setting up a simulation instance with
/// a simple interface.
///
/// Such spawned simulation still needs to be initialized with a model. See
/// `SimHandle::initialize` or `SimHandle::spawn_from`.
pub async fn spawn() -> Result<SimHandle> {
    spawn_with(SimConfig::default(), CancellationToken::new()).await
}

/// Spawns a local simulation instance based on provided configuration.
pub async fn spawn_with(config: SimConfig, cancel: CancellationToken) -> Result<SimHandle> {
    // Spawn the leader task.
    let mut leader_handle = leader::spawn(config.leader.clone(), cancel.clone())?;

    // Spawn worker tasks.
    // TODO: enable worker to worker connections, targetting a mesh topology.
    // Currently we only support star topology where all workers are connected
    // only to the leader.
    let mut workers = FnvHashMap::default();
    for _ in 0..config.worker_count {
        let worker_handle = worker::spawn(config.worker.clone(), cancel.clone())?;

        // Make sure leader and worker can talk to each other.
        leader_handle
            .connect_to_local_worker(&worker_handle, true)
            .await?;

        let worker_id = worker_handle.id().await?;

        workers.insert(worker_id, worker_handle);
    }

    // Create the sim handle.
    let mut handle = SimHandle {
        workers,
        leader: leader_handle,
        server: None,
        cancel: cancel.clone(),
        config: config.clone(),
    };

    // Optionally spawn the server task.
    //
    // NOTE: Resulting server handle can be used to send requests to the server
    // using the client API. The CTL interface is also available.
    if let Some(server_config) = config.server {
        // Extract a handle to a single worker to connect the server to.
        let worker = handle.workers.values().next().unwrap();

        // Spawn the server task.
        let mut server_handle = server::spawn(server_config, cancel.clone())?;

        // Connect the server to the worker.
        server_handle.connect_to_worker(&worker, true).await?;

        // Register as a new client connecting to the server.
        // TODO: move this to within the server handle implementation.
        let resp = server_handle
            .execute(Message::RegisterClientRequest(RegisterClientRequest {
                name: "sim_handle".to_string(),
                is_blocking: false,
                auth_token: None,
                encodings: vec![],
                transports: vec![],
            }))
            .await?;

        // Save the returned client id for use when querying the server.
        let client_id = if let Message::RegisterClientResponse(RegisterClientResponse {
            client_id,
            encoding,
            transport,
            redirect_to,
        }) = resp
        {
            Uuid::from_str(&client_id).unwrap()
        } else {
            return Err(Error::UnexpectedResponse(format!(
                "Client registration failed for local sim instance, got response: {:?}",
                resp
            )));
        };
        server_handle.client_id = Some(client_id);

        handle.server = Some(server_handle);
    }

    Ok(handle)
}

/// Convenience function for spawning simulation from provided model.
pub async fn spawn_from_model(model: Model) -> Result<SimHandle> {
    spawn_from(model, None, SimConfig::default(), CancellationToken::new()).await
}

/// Spawns a new local simulation instance using provided arguments.
pub async fn spawn_from(
    model: Model,
    scenario: Option<&str>,
    config: SimConfig,
    cancel: CancellationToken,
) -> Result<SimHandle> {
    // Spawn a raw simulation instance.
    let sim_handle = spawn_with(config, cancel).await?;

    // Initialize the simulation using provided model.
    sim_handle.pull_model(model).await?;
    sim_handle
        .initialize_with_scenario(scenario.map(|s| s.to_owned()))
        .await?;

    Ok(sim_handle)
}

/// Convenience function for spawning simulation from provided path to model
/// and optionally applying scenario selected by name.
pub async fn spawn_from_path(
    model_path: PathBuf,
    scenario: Option<&str>,
    config: SimConfig,
    cancel: CancellationToken,
) -> Result<SimHandle> {
    // TODO: move the path handling business to `Model::from_path` or similar.
    let current_path = std::env::current_dir().expect("failed getting current dir path");
    let path_buf = PathBuf::from(model_path);
    let path_to_model = current_path.join(path_buf);
    let model = Model::from_files(&vfs::PhysicalFS::new(path_to_model), None)?;

    spawn_from(model, scenario, config, cancel).await
}

/// Local simulation instance handle.
#[derive(Clone)]
pub struct SimHandle {
    pub workers: FnvHashMap<Uuid, worker::Handle>,

    pub leader: leader::Handle,

    pub server: Option<server::Handle>,

    pub cancel: CancellationToken,

    config: SimConfig,
}

impl SimHandle {
    /// Convenience method for returning a single worker that can be used as
    /// the entry point for interfacing with the simulation.
    pub fn worker(&self) -> Result<(&Uuid, &worker::Handle)> {
        if let Some(worker) = self.workers.iter().next() {
            Ok(worker)
        } else {
            Err(Error::NoAvailableWorkers)
        }
    }

    /// Convenience method for creating a new local sim instance and
    /// populating it with data from the current one.
    pub async fn fork(&mut self) -> Result<SimHandle> {
        let mut forked = spawn_from(
            self.get_model().await?,
            None,
            self.config.clone(),
            CancellationToken::new(),
        )
        .await?;

        // TODO: provide an optimized interface for carrying over entity data
        // en masse.
        // TODO: provide ability to only carry over a subset of all entities.
        // This could perhaps be done with a set of regular queries.
        for entity_name in self.entities().await? {
            let entity = self.entity(entity_name.clone()).await?;
            forked.pull_entity(entity_name, entity).await?;
        }

        Ok(forked)
    }

    /// Pulls a new config and broadcasts to all relevant parties.
    pub async fn pull_config(&mut self, config: SimConfig) -> Result<()> {
        // Broadcast to workers.
        for (_, worker) in &self.workers {
            worker
                .ctl
                .execute(Signal::from(
                    rpc::worker::Request::PullConfig(config.worker.clone()).into_local(),
                ))
                .await??
                .discard_context()
                .ok()?;
        }

        // TODO: send config to leader.
        // TODO: send config to server.

        Ok(())
    }

    /// Access the client-based interface.
    ///
    /// By default `SimHandle` will interface with cluster participants
    /// directly, bypassing the client-server level.
    pub fn as_client(self) -> SimClient {
        SimClient(self)
    }

    /// Registers new machine for instancing based on requirements.
    // TODO: provide machine instruction set as argument
    // TODO: require instancing target rules.
    #[cfg(feature = "machine")]
    pub async fn register_machine(&mut self) -> Result<()> {
        // self.worker
        //     .ctl
        //     .execute(rpc::worker::Request::RegisterMachine())
        //     .await?;
        todo!()
    }

    /// Spawns a new machine task.
    #[cfg(feature = "machine")]
    pub async fn spawn_machine(
        &mut self,
        behavior_script: Option<String>,
        triggers: Vec<EventName>,
    ) -> Result<MachineHandle> {
        let machine = crate::machine::spawn(
            behavior_script,
            triggers,
            self.worker()?.1.behavior_exec.clone(),
        )?;
        Ok(machine)
    }

    /// Spawns a new synced behavior task based on the provided closure.
    ///
    /// Note that this function doesn't provide a way for identifying the behavior
    /// down the line, storing the name as empty string.
    ///
    /// Behavior handle is stored internally, so that simulation events can be
    /// properly propagated. Another handle is returned to the caller.
    ///
    /// Takes in an optional collection of triggers. `None` means a continuous
    /// behavior without explicit external triggering.
    pub async fn spawn_behavior_synced(
        &mut self,
        f: impl FnOnce(
            tokio_stream::wrappers::ReceiverStream<(
                Signal<rpc::behavior::Request>,
                tokio::sync::oneshot::Sender<Result<Signal<rpc::behavior::Response>>>,
            )>,
            LocalExec<Signal<rpc::worker::Request>, Result<Signal<rpc::worker::Response>>>,
        ) -> BoxFuture<'static, Result<()>>,
        target: BehaviorTarget,
        triggers: Vec<EventName>,
    ) -> Result<behavior::BehaviorHandle> {
        let handle = behavior::spawn_synced(
            f,
            "".to_owned(),
            triggers,
            self.worker()?.1.behavior_exec.clone(),
        )?;

        self.worker()?
            .1
            .ctl
            .execute(Signal::from(rpc::worker::RequestLocal::AddBehavior(
                target,
                handle.clone(),
            )))
            .await?;

        Ok(handle)
    }

    pub async fn spawn_behavior_unsynced(
        &mut self,
        f: impl FnOnce(
            tokio::sync::broadcast::Receiver<rpc::behavior::Request>,
            LocalExec<Signal<rpc::worker::Request>, crate::Result<Signal<rpc::worker::Response>>>,
        ) -> BoxFuture<'static, Result<()>>,
    ) -> Result<()> {
        behavior::spawn_unsynced(
            f,
            self.worker()?.1.behavior_broadcast.subscribe(),
            self.worker()?.1.behavior_exec.clone(),
        )?;

        Ok(())
    }

    /// Broadcasts an event across the simulation.
    pub async fn invoke(&mut self, event: &str) -> Result<()> {
        self.worker()?
            .1
            .ctl
            .execute(Signal::from(
                rpc::worker::Request::Invoke {
                    events: vec![event.to_owned()],
                    global: true,
                }
                .into_local(),
            ))
            .await??;
        Ok(())
    }

    /// Advances the simulation by a single step.
    ///
    /// # Optional nature of synchronization
    ///
    /// Stepping the simulation simply means emitting a simulation-wide `step`
    /// event and incrementing the internal clock.
    ///
    /// Synchronization is not required however. It's possible that none of the
    /// `behavior`s, `server`s, etc. that are part of the current simulation
    /// setup will choose to observe and/or act upon `step` events.
    ///
    /// It's also possible that it will be a mixed bag, some parts of the
    /// system can make use of synchronization and others can remain
    /// "real-time".
    pub async fn step(&mut self) -> Result<()> {
        self.leader
            .ctl
            .execute(Signal::from(rpc::leader::RequestLocal::Request(
                rpc::leader::Request::Step,
            )))
            .await??;

        Ok(())
    }

    /// Advances the simulation by multiple steps.
    pub async fn step_by(&mut self, step_count: usize) -> Result<()> {
        for n in 0..step_count {
            self.step().await?;
        }
        Ok(())
    }

    pub async fn entity(&mut self, name: EntityName) -> Result<Entity> {
        let sig = self
            .worker()?
            .1
            .ctl
            .execute(Signal::from(
                rpc::worker::Request::GetEntity(name).into_local(),
            ))
            .await??;

        match sig.discard_context() {
            rpc::worker::Response::Entity(entity) => Ok(entity),
            _ => unimplemented!(),
        }
    }

    pub async fn pull_entity(&mut self, name: EntityName, entity: Entity) -> Result<()> {
        let sig = self
            .worker()?
            .1
            .ctl
            .execute(Signal::from(
                rpc::worker::Request::TakeEntity(name, entity).into_local(),
            ))
            .await??;

        match sig.discard_context() {
            rpc::worker::Response::Empty => Ok(()),
            _ => unimplemented!(),
        }
    }

    pub async fn entities(&mut self) -> Result<Vec<EntityName>> {
        let sig = self
            .worker()?
            .1
            .ctl
            .execute(Signal::from(rpc::worker::Request::EntityList.into_local()))
            .await??;

        match sig {
            Signal {
                payload: rpc::worker::Response::EntityList(entities),
                ..
            } => Ok(entities),
            _ => Err(Error::UnexpectedResponse(format!(
                "expected worker::Response::EntityList, got {:?}",
                sig
            ))),
        }
    }

    /// Propagates the provided model to cluster participants.
    pub async fn pull_model(&self, model: Model) -> Result<()> {
        use rpc::leader::{Request, RequestLocal, Response};
        self.leader
            .ctl
            .execute(Signal::from(RequestLocal::Request(Request::ReplaceModel(
                model,
            ))))
            .await?
            .map(|_| ())
    }

    /// Initializes simulation state using the loaded model.
    pub async fn initialize(&self) -> Result<()> {
        self.initialize_with_scenario(None).await
    }

    /// Initializes simulation state using the loaded model and optional
    /// scenario name.
    ///
    /// Scenario is effectively a set of additional initialization rules
    /// defined at the model level.
    pub async fn initialize_with_scenario(&self, scenario: Option<String>) -> Result<()> {
        use rpc::leader::{Request, RequestLocal, Response};

        self.leader
            .ctl
            .execute(Signal::from(RequestLocal::Request(Request::Initialize {
                scenario,
            })))
            .await?
            .map(|_| ())
    }

    /// Initiates loading of the provided snapshot onto the cluster.
    ///
    /// This operation differs from regular initialization, in that there's
    /// not only the model data, but also the actual entity state data that
    /// needs to be spread across cluster participants.
    pub async fn load_snapshot(&self, path: String) -> Result<()> {
        unimplemented!()
    }

    /// Spawns an entity.
    pub async fn spawn_entity(&self, name: EntityName, prefab: Option<PrefabName>) -> Result<()> {
        let req: rpc::leader::RequestLocal =
            rpc::leader::Request::SpawnEntity { name, prefab }.into();
        let resp = self.leader.ctl.execute(Signal::from(req)).await??;
        match resp.discard_context() {
            rpc::leader::Response::Empty => Ok(()),
            resp => Err(Error::UnexpectedResponse(format!("{resp}"))),
        }
    }

    /// Spawns multiple entities.
    pub async fn spawn_entities(&self, prefab: Option<PrefabName>, count: usize) -> Result<()> {
        for n in 0..count {
            let ctl = self.leader.ctl.clone();
            let req: rpc::leader::RequestLocal = rpc::leader::Request::SpawnEntity {
                name: Uuid::new_v4().simple().to_string(),
                prefab: prefab.clone(),
            }
            .into();
            tokio::spawn(async move {
                let resp = ctl.execute(Signal::from(req)).await??;
                match resp.discard_context() {
                    rpc::leader::Response::Empty => Ok(()),
                    resp => Err(Error::UnexpectedResponse(format!("{resp}"))),
                }
            });
        }
        Ok(())
    }

    /// Removes an entity, destroying both the entity object and any related
    /// behavior tasks.
    pub async fn remove_entity(&self, name: &str) -> Result<()> {
        let sig = self
            .leader
            .ctl
            .execute(Signal::from(rpc::leader::RequestLocal::Request(
                rpc::leader::Request::RemoveEntity {
                    name: name.to_owned(),
                },
            )))
            .await??;
        match sig {
            Signal {
                payload: rpc::leader::Response::Empty,
                ..
            } => Ok(()),
            _ => Err(Error::UnexpectedResponse(format!("{}", sig.payload))),
        }
    }

    /// Retrieves the model currently loaded on the cluster.
    pub async fn get_model(&mut self) -> Result<Model> {
        Ok(self
            .leader
            .ctl
            .execute(Signal::from(rpc::leader::RequestLocal::Request(
                rpc::leader::Request::Model,
            )))
            .await??
            .discard_context()
            .try_into()?)
    }

    /// Queries the connected worker using the regular query definition.
    pub async fn query(&self, query: crate::Query) -> Result<crate::QueryProduct> {
        Ok(self
            .worker()?
            .1
            .ctl
            .execute(
                Signal::from(rpc::worker::Request::ProcessQuery(query).into_local())
                    .originating_at(rpc::Caller::SimHandle),
            )
            .await??
            .discard_context()
            .try_into()?)
    }

    pub async fn subscribe(
        &self,
        triggers: Vec<query::Trigger>,
        query: crate::Query,
    ) -> Result<crate::executor::Receiver<Result<Signal<rpc::worker::Response>>>> {
        let rcv = self
            .worker()?
            .1
            .ctl
            .execute_to_multi(Signal::from(
                rpc::worker::Request::Subscribe(triggers, query).into_local(),
            ))
            .await?;
        Ok(rcv)
    }

    /// Queries the cluster for a single value at the specified address.
    pub async fn get_var(&self, addr: Address) -> Result<Var> {
        let var = self
            .worker()?
            .1
            .query(
                Query::default()
                    .filter(query::Filter::Name(vec![addr.entity.clone()]))
                    .map(query::Map::SelectAddrs(vec![addr])),
                Some(rpc::Caller::SimHandle),
            )
            .await?
            .to_var()?;
        Ok(var)
    }

    /// Asks the cluster to set variable at the specified address to the
    /// provided value.
    pub async fn set_var(&self, addr: Address, var: Var) -> Result<()> {
        self.worker()?
            .1
            .ctl
            .execute(Signal::from(
                rpc::worker::Request::SetVar(addr, var).into_local(),
            ))
            .await??;
        Ok(())
    }

    /// Gets the current value of the simulation clock.
    pub async fn get_clock(&self) -> Result<u64> {
        let sig = self
            .leader
            .execute(Signal::from(rpc::leader::Request::Clock))
            .await??;
        if let Signal {
            payload: rpc::leader::Response::Clock(clock),
            ..
        } = sig
        {
            Ok(clock)
        } else {
            unimplemented!()
        }
    }

    /// Triggers reorganizing of entities across the cluster.
    ///
    /// Normally this is done continuously based on cluster config, with
    /// entity-level granularity and depending on multiple tracked variables.
    ///
    /// Calling this function initiates a recalculation for all entities and
    /// potentially subsequent migrations.
    ///
    // HACK: the shuffle flag provides a quick way for forcefully migrating
    // all the entities to random cluster workers.
    pub async fn reorganize(&self, shuffle: bool) -> Result<()> {
        if shuffle {
            self.leader
                .execute(Signal::from(rpc::leader::Request::ReorganizeEntities {
                    shuffle,
                }))
                .await??;
        }

        Ok(())
    }

    /// Initiates proper shutdown by propagating the shutdown signal across
    /// all running tasks.
    pub async fn shutdown(&self) -> Result<()> {
        trace!("initiating shutdown on local sim instance");
        // self.leader.shutdown().await?;
        // self.worker.shutdown().await?;
        if let Some(server) = &self.server {
            // server.shutdown().await?;
        }
        self.cancel.cancel();
        Ok(())
    }

    /// Pulls entity data and creates a snapshot.
    pub async fn snapshot(&mut self) -> Result<Snapshot> {
        let entities = Default::default();

        let snap = Snapshot {
            created_at: chrono::Utc::now().timestamp() as u64,
            // TODO: count all the workers, also the ones that connected from
            // the outside and don't reside within the same process.
            worker_count: self.workers.len() as u32,
            clock: self.get_clock().await?,
            model: self.get_model().await?,
            entities,
        };

        Ok(snap)
    }
}

/// Newtype encapsulating the simulation handle for the purpose of implementing
/// the client-based interface for it.
#[derive(Clone)]
pub struct SimClient(SimHandle);

#[async_trait]
impl AsyncClient for SimClient {
    type Client = Self;

    async fn connect(addr: CompositeAddress, config: client::Config) -> Result<Self::Client> {
        todo!()
    }

    async fn shutdown(&mut self) -> Result<()> {
        debug!("initiating shutdown on local sim instance");
        self.0.shutdown().await?;
        self.0.cancel.cancel();
        Ok(())
    }

    async fn is_alive(&mut self) -> Result<()> {
        Ok(())
    }

    async fn initialize(&mut self) -> Result<()> {
        self.0.initialize().await
    }

    async fn spawn_entity(&mut self, name: EntityName, prefab: Option<PrefabName>) -> Result<()> {
        self.0.spawn_entity(name, prefab).await
    }
    async fn despawn_entity(&mut self, name: &str) -> Result<()> {
        self.0.remove_entity(name).await
    }

    async fn get_model(&mut self) -> Result<Model> {
        self.0.get_model().await
    }

    async fn status(&mut self) -> Result<rpc::msg::StatusResponse> {
        if let Some(server) = &self.0.server {
            let resp = server
                .execute(Message::StatusRequest(msg::StatusRequest {
                    format: format!(""),
                }))
                .await?;
            if let Message::StatusResponse(status) = resp {
                Ok(status)
            } else {
                Err(Error::UnexpectedResponse(format!("")))
            }
        } else {
            Err(Error::Other(format!(
                "sim client: `server` not available on the sim handler"
            )))
        }
    }

    async fn step(&mut self, step_count: u32) -> Result<()> {
        // self.0
        //     .server
        //     .as_ref()
        //     .ok_or(Error::Other(format!("no server")))?
        //     .execute(Message::AdvanceRequest(AdvanceRequest {
        //         step_count,
        //         wait: true,
        //     }))
        //     .await??;
        self.0.step().await?;
        Ok(())
    }

    async fn invoke(&mut self, event: &str) -> Result<()> {
        self.0.invoke(event).await
    }

    async fn query(&mut self, q: Query) -> Result<crate::QueryProduct> {
        self.0.query(q).await
    }

    async fn subscribe(
        &mut self,
        triggers: Vec<query::Trigger>,
        query: Query,
    ) -> Result<crate::executor::Receiver<Message>> {
        if let Some(server) = &self.0.server {
            server
                .client
                .execute_to_multi((
                    Some(Uuid::nil()),
                    Message::SubscribeRequest(triggers, query),
                ))
                .await
        } else {
            Err(Error::ServerNotConnected(
                "sim handle doesn't have the server interface available".to_owned(),
            ))
        }
    }

    async fn unsubscribe(&mut self, subscription_id: Uuid) -> Result<()> {
        // self.0
        //     .workers
        //     .ctl
        //     .execute(Signal::from(
        //         rpc::worker::Request::Unsubscribe(subscription_id).into_local(),
        //     ))
        //     .await??;
        // Ok(())

        if let Some(server) = &self.0.server {
            server
                .client
                .execute((None, Message::UnsubscribeRequest(subscription_id)))
                .await?
                .ok()
        } else {
            Err(Error::ServerNotConnected(
                "sim handle doesn't have the server interface available".to_owned(),
            ))
        }
    }

    async fn snapshot(&mut self) -> Result<Snapshot> {
        if let Some(server) = &self.0.server {
            server
                .client
                .execute((Some(Uuid::nil()), Message::SnapshotRequest))
                .await?
                .try_into()
        } else {
            Err(Error::ServerNotConnected(
                "sim handle doesn't have the server interface available".to_owned(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio_util::sync::CancellationToken;

    use crate::rpc::msg::Message;
    use crate::sim::SimConfig;
    use crate::{server, sim, Executor, Result};

    #[tokio::test]
    async fn server_ping() -> Result<()> {
        let handle = sim::spawn_with(
            SimConfig {
                server: Some(server::Config::default()),
                ..Default::default()
            },
            CancellationToken::new(),
        )
        .await?;

        let response = handle
            .server
            .unwrap()
            .execute(Message::PingRequest(vec![1; 5]))
            .await?;

        assert_eq!(response, Message::PingResponse(vec![1; 5]));

        Ok(())
    }
}
