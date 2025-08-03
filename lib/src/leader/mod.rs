#![allow(unused)]

mod manager;

use std::borrow::Borrow;
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::ops::DerefMut;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;
use std::{io, thread};

use chrono::{DateTime, Utc};
use fnv::FnvHashMap;
use id_pool::IdPool;
use rand::SeedableRng;
use tokio::sync::oneshot::Sender;
use tokio::sync::{oneshot, Mutex};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

// use bigworlds::msg::{Message, MessageType};
use crate::error::{Error, Result};
use crate::executor::{Executor, LocalExec, RemoteExec, Signal};
use crate::leader::manager::ManagerExec;
use crate::net::{CompositeAddress, ConnectionOrAddress, Encoding, Transport};
use crate::rpc::leader::{Request, RequestLocal, Response};
use crate::rpc::Caller;
use crate::util_net::{decode, encode};
use crate::worker::{WorkerExec, WorkerId, WorkerRemoteExec};
use crate::{model, net, query, rpc, string, worker, EntityId, EntityName, Model, QueryProduct};

const LEADER_ADDRESS: &str = "0.0.0.0:5912";

pub type LeaderId = Uuid;

/// Single worker as seen by the leader.
#[derive(Clone, Debug)]
pub struct Worker {
    pub my_id: LeaderId,

    pub id: WorkerId,

    /// Tracked list of entities that currently exist on the worker.
    pub entities: Vec<EntityName>,

    /// Executor abstracting over the transmission medium.
    exec: WorkerExec,

    /// List of known listeners the worker can be reached through.
    listeners: Vec<CompositeAddress>,

    /// Information about worker synchronization situation. Workers with
    /// attached servers can block processing of further steps if any of their
    /// connected clients blocks.
    pub is_blocking_step: bool,
    pub furthest_agreed_step: usize,
}

impl Worker {
    pub fn new(my_id: LeaderId, id: WorkerId, exec: WorkerExec) -> Self {
        Self {
            my_id,
            id,
            entities: vec![],
            exec,
            listeners: vec![],
            is_blocking_step: false,
            furthest_agreed_step: 0,
        }
    }
}

#[async_trait::async_trait]
impl Executor<Signal<rpc::worker::Request>, Signal<rpc::worker::Response>> for Worker {
    async fn execute(
        &self,
        sig: Signal<rpc::worker::Request>,
    ) -> Result<Signal<rpc::worker::Response>> {
        match &self.exec {
            WorkerExec::Remote(remote_exec) => {
                remote_exec
                    .execute(Signal::from(sig).originating_at(Caller::Worker(self.my_id)))
                    .await?
            }
            WorkerExec::Local(local_exec) => {
                let sig = Signal::new(sig.payload.into(), sig.ctx);
                local_exec
                    .execute(sig)
                    .await
                    .map_err(|e| Error::Other(e.to_string()))?
                    .map_err(|e| Error::Other(e.to_string()))
            }
        }
    }
}

/// Leader holds simulation's central authority struct and manages
/// a network of workers.
///
/// It doesn't hold any entity state, leaving that entirely to workers.
pub struct State {
    pub id: LeaderId,

    /// Starting configuration.
    pub config: Config,

    /// Map of connected workers by their id.
    pub workers: FnvHashMap<WorkerId, Worker>,
    /// Map of worker addresses to worker ids.
    pub workers_by_addr: FnvHashMap<SocketAddr, WorkerId>,

    /// Currently loaded model.
    pub model: Option<Model>,

    /// Current simulation clock.
    pub clock: usize,

    pub status: Status,
}

/// Information about the state of the leader itself.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Status {
    /// Exact time when the leader was spawned.
    pub started_at: DateTime<Utc>,
}

impl Status {
    pub fn new() -> Self {
        Self {
            started_at: Utc::now(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub listeners: Vec<CompositeAddress>,

    /// Perform automatic stepping through the simulation, initiated on the
    /// leader level.
    pub autostep: Option<std::time::Duration>,

    /// Policy for entity distribution. Most policies involve a dynamic process
    /// of reassigning entities between workers to optimize for different
    /// factors.
    pub distribution: DistributionPolicy,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listeners: vec![],
            autostep: None,
            distribution: DistributionPolicy::Random,
        }
    }
}

/// Execution handle for sending requests to leader.
///
/// # Worker identification
///
/// The executor implementation for this type must send a `worker_id` to
/// identify with leader. This type stores a `worker_id` to use for
/// subsequent requests.
///
/// We can also supply any other `worker_id` but then we must go through
/// the `worker_exec` and not the executor implemented directly on
/// `LeaderHandle`.
#[derive(Clone)]
pub struct Handle {
    pub ctl: LocalExec<Signal<rpc::leader::RequestLocal>, Result<Signal<rpc::leader::Response>>>,

    /// Worker executor for running requests coming from a local worker
    pub worker_exec:
        LocalExec<Signal<rpc::leader::RequestLocal>, Result<Signal<rpc::leader::Response>>>,
    // pub worker_id: Option<WorkerId>,
}

#[async_trait::async_trait]
impl Executor<Signal<rpc::leader::Request>, Result<Signal<rpc::leader::Response>>> for Handle {
    async fn execute(
        &self,
        sig: Signal<rpc::leader::Request>,
    ) -> Result<Result<Signal<rpc::leader::Response>>> {
        let sig = Signal::new(sig.payload.into(), sig.ctx);
        self.ctl.execute(sig).await.map_err(|e| e.into())
    }
}

impl Handle {
    /// Connects leader to remote worker.
    pub async fn connect_to_remote_worker(&mut self, addr: CompositeAddress) -> Result<()> {
        let req: RequestLocal = Request::ConnectToWorker(addr).into();
        self.ctl.execute(Signal::from(req)).await??;
        Ok(())
    }

    /// Connects leader to worker existing on the same runtime.
    ///
    /// It also makes sure to connect worker to leader. Local channel
    /// communications are more difficult and we need to set things up both
    /// ways.
    pub async fn connect_to_local_worker(
        &mut self,
        worker_handle: &worker::Handle,
        duplex: bool,
    ) -> Result<()> {
        // Issue this leader to reach out with a connection to the worker.
        self.ctl
            .execute(Signal::from(rpc::leader::RequestLocal::ConnectToWorker(
                worker_handle.leader_exec.clone(),
                self.worker_exec.clone(),
            )))
            .await??;

        if duplex {
            // Prompt the worker to connect to this leader as well.
            worker_handle.connect_to_local_leader(self).await?;
        }

        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.ctl
            .execute(Signal::from(RequestLocal::Shutdown))
            .await??;
        Ok(())
    }
}

/// Spawns a `Leader` task on the current runtime.
///
/// Leader serves as cluster's central authority and manages a network
/// of workers. It orchestrates centralized operations such as entity spawning
/// and distribution.
///
/// # Interfaces
///
/// Leader provides a standard network interface for workers to connect
/// to. It also exposes a worker executor that can be used by local worker
/// tasks.
pub fn spawn(config: Config, mut cancel: CancellationToken) -> Result<Handle> {
    let cancel = cancel.child_token();

    let (local_ctl_executor, mut local_ctl_stream, _) =
        LocalExec::<Signal<RequestLocal>, Result<Signal<Response>>>::new(20);
    let (local_worker_executor, mut local_worker_stream, _) =
        LocalExec::<Signal<RequestLocal>, Result<Signal<Response>>>::new(20);
    let (net_exec, mut net_stream, _) = LocalExec::new(20);
    net::spawn_listeners(&config.listeners, net_exec.clone(), cancel.clone())?;

    debug!("spawning leader task, listeners: {:?}", config.listeners);

    let autostep = config.autostep.clone();
    // println!("autostep: {:?}", autostep);
    let mut state = State {
        id: Uuid::new_v4(),
        config,
        clock: 0,
        model: None,
        workers: Default::default(),
        workers_by_addr: Default::default(),
        status: Status::new(),
    };
    let manager = manager::spawn(state, cancel.clone())?;

    let _ctl_exec = local_ctl_executor.clone();
    tokio::spawn(async move {
        //
    });

    let _ctl_exec = local_ctl_executor.clone();
    tokio::spawn(async move {
        // Trigger auto-steps in a separate task.
        if let Some(autostep_delta) = autostep {
            let cancel_c = cancel.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;
                loop {
                    tokio::time::sleep(autostep_delta).await;
                    if cancel_c.is_cancelled() {
                        // println!("leader got cancelled, shutting down auto-step task");
                        break;
                    }
                    match _ctl_exec
                        .execute(Signal::from(rpc::leader::RequestLocal::Request(
                            rpc::leader::Request::Step,
                        )))
                        .await
                    {
                        Ok(_) => {
                            debug!("leader processed step");
                            continue;
                        }
                        Err(e) => match e {
                            Error::TokioOneshotRecvError(e) => return,
                            _ => {
                                warn!("leader failed processing step: {:?}", e);
                                break;
                            }
                        },
                    }
                }
            });
        }

        loop {
            let manager = manager.clone();

            tokio::select! {
                Some((sig, s)) = local_ctl_stream.next() => {
                    debug!("leader: processing local controller request");
                    tokio::spawn(async move {
                        let resp = handle_local_controller_request(sig.payload, sig.ctx, manager).await;
                        s.send(resp);
                    });
                },
                Some((sig, s)) = local_worker_stream.next() => {
                    trace!("leader: processing message from local worker");
                    tokio::spawn(async move {
                        let resp = handle_local_worker_request(sig.payload, sig.ctx, manager).await;
                        s.send(resp);
                    });

                }
                Some(((maybe_con, bytes), s)) = net_stream.next() => {
                    tokio::spawn(async move {
                        let sig: Signal<Request> = match decode(&bytes, Encoding::Bincode) {
                            Ok(r) => r,
                            Err(e) => {
                                error!("failed decoding request (bincode)");
                                return;
                            }
                        };
                        debug!("leader: processing request from network: {sig:?}");
                        let resp = handle_network_request(sig.payload, sig.ctx, manager.clone(), maybe_con).await;
                        debug!("leader: response: {resp:?}");
                        s.send(encode(resp, Encoding::Bincode).unwrap()).unwrap();
                    });
                }
                _ = cancel.cancelled() => break,
            }
        }
    });

    Ok(Handle {
        ctl: local_ctl_executor,
        worker_exec: local_worker_executor,
    })
}

async fn handle_local_worker_request(
    req: rpc::leader::RequestLocal,
    ctx: Option<rpc::Context>,
    // worker_id: WorkerId,
    manager: ManagerExec,
) -> Result<Signal<rpc::leader::Response>> {
    use rpc::leader::{RequestLocal, Response};
    match req {
        RequestLocal::ConnectAndRegisterWorker(executor) => {
            let worker_id = if let Some(ctx) = &ctx {
                if let Caller::Worker(id) = ctx.origin {
                    id
                } else {
                    return Err(Error::Other("Signal origin is not worker".to_string()));
                }
            } else {
                return Err(Error::Other("Unable to get context".to_string()));
            };

            let my_id = manager.get_meta().await?;
            let worker = Worker::new(my_id, worker_id, WorkerExec::Local(executor));

            // TODO: rework; we don't really need to get information about
            // other workers.
            let resp = manager.execute(manager::Request::GetWorkers).await;

            match resp {
                Ok(resp) => match resp {
                    Ok(resp) => {
                        if let manager::Response::Workers(workers) = resp {
                            trace!(
                                "leader: ConnectAndRegisterWorker: current number of workers: {}",
                                workers.len()
                            );
                        } else {
                            unimplemented!()
                        };

                        manager
                            .execute(manager::Request::AddWorker(worker))
                            .await
                            .unwrap();

                        Ok(Signal::new(Response::Empty, ctx))
                    }
                    Err(e) => {
                        error!("{}", e.to_string());
                        Err(e)
                    }
                },
                Err(e) => {
                    error!("{}", e.to_string());
                    Err(e.into())
                }
            }
        }
        RequestLocal::Request(req) => {
            debug!("req: {:?}", req);
            handle_worker_request(req, ctx, manager).await
        }
        _ => todo!(),
    }
}

pub async fn handle_worker_request(
    req: rpc::leader::Request,
    ctx: Option<rpc::Context>,
    leader: ManagerExec,
) -> Result<Signal<rpc::leader::Response>> {
    use rpc::leader::{Request, Response};

    match req {
        Request::ReplaceModel(model) => {
            // debug!(
            //     "pulling model, bytes: {}",
            //     bincode::serialize(&model)?.len()
            // );
            unimplemented!();
            // leader.lock().await.project = Some(project);
            Ok(Signal::new(rpc::leader::Response::ReplaceModel, ctx))
        }
        Request::Ping(bytes) => Ok(Signal::new(Response::Ping(bytes), ctx)),
        Request::Model => {
            let model = leader.get_model().await?;
            Ok(Signal::new(Response::Model(model), ctx))
        }
        // Request::MemorySize => {
        //     println!("memory size unknown");
        // }
        // Request::Entities => {
        //     let mut entities = vec![];
        //     for (worker_id, worker) in &leader.workers {
        //         entities.extend(worker.entities.iter());
        //     }
        //     s.send(Ok(Response::Entities {
        //         machined: entities,
        //         non_machined: vec![],
        //     }));
        // }
        Request::ReadyUntil(target_clock) => {
            unimplemented!();
            // if let Some(worker) = leader.lock().await.workers.get_mut(&worker_id) {
            //     worker.furthest_agreed_step = target_clock;
            // } else {
            //     unimplemented!()
            // }
            Ok(Signal::new(Response::Empty, ctx))
        }
        _ => handle_request(req, ctx, leader).await,
    }
}

async fn handle_local_controller_request(
    req: rpc::leader::RequestLocal,
    ctx: Option<rpc::Context>,
    manager: ManagerExec,
) -> Result<Signal<rpc::leader::Response>> {
    match req {
        rpc::leader::RequestLocal::ConnectToWorker(leader_worker, worker_leader) => {
            let mut resp = leader_worker
                .execute(Signal::new(
                    rpc::worker::RequestLocal::IntroduceLeader(worker_leader),
                    ctx,
                ))
                .await?;
            match resp {
                Ok(Signal { ctx, .. }) => Ok(Signal::new(rpc::leader::Response::Empty, ctx)),
                Ok(Signal { ctx, .. }) => Err(Error::UnexpectedResponse("".to_string())),
                Err(e) => Err(Error::FailedConnectingLeaderToWorker(e.to_string())),
            }
        }
        rpc::leader::RequestLocal::Request(req) => {
            handle_controller_request(req, ctx, manager).await
        }
        RequestLocal::ConnectToWorker(local_exec, local_exec1) => todo!(),
        RequestLocal::ConnectAndRegisterWorker(local_exec) => todo!(),
        RequestLocal::Request(request) => todo!(),
        RequestLocal::Shutdown => {
            // manager.shutdown().await?;
            Ok(Signal::new(Response::Empty, ctx))
        }
    }
}

async fn handle_controller_request(
    req: rpc::leader::Request,
    ctx: Option<rpc::Context>,
    manager: ManagerExec,
) -> Result<Signal<rpc::leader::Response>> {
    trace!("leader: processing controller request: {req:?}");
    match req {
        // Pull in a new project into the cluster and load it as the current
        // project, replacing the last project.
        Request::ReplaceModel(model) => {
            manager.replace_model(model).await?;
            Ok(Signal::new(rpc::leader::Response::Empty, ctx))
        }
        // Initialize the cluster using provided scenario. Scenario must be
        // present in the currently loaded project.
        Request::Initialize { scenario } => {
            let model = manager.get_model().await?;
            initialize(model, manager).await?;
            Ok(Signal::new(rpc::leader::Response::Empty, ctx))
        }
        // Initialize processing a single step.
        Request::Step => {
            process_step(manager).await?;
            Ok(Signal::new(rpc::leader::Response::Empty, ctx))
        }
        // Request::ConnectToWorker(address) => {}
        _ => handle_request(req, ctx, manager).await,
    }
}

/// Handler containing additional net caller information.
async fn handle_network_request(
    req: rpc::leader::Request,
    ctx: Option<rpc::Context>,
    manager: ManagerExec,
    maybe_con: ConnectionOrAddress,
) -> Result<Signal<rpc::leader::Response>> {
    match req {
        Request::IntroduceWorker(id) => {
            let exec = match maybe_con {
                ConnectionOrAddress::Connection(con) => RemoteExec::new(con),
                ConnectionOrAddress::Address(_) => {
                    // TODO: establish a new connection to the provided
                    // address.
                    todo!();
                }
            };
            let worker = Worker {
                my_id: manager.get_meta().await?,
                id,
                entities: vec![],
                exec: WorkerExec::Remote(exec),
                // TODO: fill in worker listeners with a subsequent request to
                // the worker.
                listeners: vec![],
                is_blocking_step: false,
                furthest_agreed_step: 0,
            };
            manager.add_worker(worker).await?;

            if let Ok(model) = manager.get_model().await {
                Ok(Signal::new(
                    Response::Model(manager.get_model().await?),
                    ctx,
                ))
            } else {
                Ok(Signal::new(Response::Empty, ctx))
            }
        }
        _ => handle_request(req, ctx, manager).await,
    }
}

async fn handle_request(
    req: rpc::leader::Request,
    ctx: Option<rpc::Context>,
    manager: ManagerExec,
) -> Result<Signal<rpc::leader::Response>> {
    match req {
        Request::Status => Ok(Signal::new(
            Response::Status(manager.get_status().await?),
            ctx,
        )),
        Request::ConnectToWorker(address) => {
            trace!("leader: connecting to worker: {:?}", address);
            let bind = SocketAddr::from_str("0.0.0.0:0")?;
            let endpoint = net::quic::make_client_endpoint_insecure(bind)
                .map_err(|e| Error::NetworkError(e.to_string()))?;
            let connection = endpoint
                .connect(address.address.clone().try_into().unwrap(), "any")?
                .await?;
            trace!("leader: connected to worker");

            let worker_remote_exec = WorkerRemoteExec::new(connection);
            trace!("leader: remote worker executor created");

            let workers = manager.get_workers().await?;
            trace!(
                "leader: connect_to_worker: current cluster view (workers): {:?}",
                workers
            );
            let resp = worker_remote_exec
                .execute(Signal::from(rpc::worker::Request::IntroduceLeader {
                    listeners: manager.get_config().await?.listeners,
                    cluster_view: workers
                        .into_iter()
                        .map(|(_, worker)| (worker.id, worker.listeners))
                        .collect::<Vec<_>>(),
                }))
                .await??
                .into_payload();
            let worker_id = match resp {
                rpc::worker::Response::IntroduceLeader { worker_id } => worker_id,
                _ => return Err(Error::UnexpectedResponse(format!("{}", resp))),
            };
            trace!("leader: sent introduction: got response: {resp:?}");

            let worker = Worker {
                my_id: manager.get_meta().await?,
                id: worker_id,
                // Worker starts out with an empty entity store.
                entities: vec![],
                // TODO: get information about any additional listeners exposed
                // by the worker. For now we only pass the address provided
                // with the request.
                listeners: vec![address],
                exec: WorkerExec::Remote(worker_remote_exec),
                is_blocking_step: false,
                furthest_agreed_step: 0,
            };

            if let rpc::worker::Response::IntroduceLeader { worker_id } = resp {
                // Store the worker executor.
                manager.add_worker(worker).await?;
            } else {
                unimplemented!("unexpected response: {:?}", resp);
            }

            Ok(Signal::new(rpc::leader::Response::Empty, ctx))
        }
        Request::DisconnectWorker => {
            if let Some(ctx) = ctx {
                manager.remove_worker(ctx.origin.id()).await?;
                println!("leader: worker disconnected");
                Ok(Signal::new(Response::Empty, Some(ctx)))
            } else {
                Err(Error::ContextRequired(format!("request: {:?}", req)))
            }
        }
        Request::ReplaceModel(model) => todo!(),
        Request::MergeModel(model) => {
            manager.merge_model(model).await?;
            Ok(Signal::new(Response::Empty, ctx))
        }
        Request::Initialize { scenario } => todo!(),
        Request::Step => todo!(),
        Request::Ping(vec) => todo!(),
        Request::Clock => Ok(Signal::new(
            Response::Clock(manager.get_clock().await?),
            ctx,
        )),
        Request::Model => Ok(Signal::new(
            Response::Model(manager.get_model().await?),
            ctx,
        )),
        Request::ReadyUntil(_) => todo!(),
        Request::SpawnEntity { name, prefab } => {
            // Spawn new entity on random worker.
            let worker = manager.get_random_worker().await?;

            debug!("spawning entity on worker: {}", worker.id);
            worker
                .execute(Signal::from(rpc::worker::Request::SpawnEntity {
                    name,
                    prefab,
                }))
                .await?;

            Ok(Signal::new(Response::Empty, ctx))
        }
        Request::RemoveEntity { name } => {
            let workers = manager.get_workers().await?;

            // Broadcast the despawning request across the cluster.
            // TODO: parallelize. Break on successful response.
            for (_, worker) in workers {
                match worker
                    .execute(Signal::from(rpc::worker::Request::DespawnEntity {
                        name: name.clone(),
                    }))
                    .await
                {
                    Err(Error::FailedGettingEntityByName(_)) => continue,
                    Ok(Signal {
                        payload: rpc::worker::Response::Empty,
                        ..
                    }) => {
                        info!(
                            "leader: despawn_entity: entity was found on worker {} and destroyed",
                            worker.id
                        );
                        break;
                    }
                    _ => (),
                }
            }

            Ok(Signal::new(Response::Empty, ctx))
        }
        Request::GetWorkers => {
            let _workers = manager.get_workers().await?;
            let mut workers = FnvHashMap::default();
            for (worker_id, worker) in _workers {
                match worker
                    .execute(Signal::from(rpc::worker::Request::GetListeners))
                    .await?
                    .into_payload()
                {
                    rpc::worker::Response::GetListeners(listeners) => {
                        workers.insert(worker_id, listeners)
                    }
                    _ => continue,
                };
            }
            Ok(Signal::new(Response::GetWorkers(workers), ctx))
        }
        Request::IntroduceWorker(uuid) => unreachable!(),
        Request::WorkerProxy(mut request) => {
            let ctx = ctx.ok_or(Error::ContextRequired(
                "Unable to process WorkerProxy request".to_string(),
            ))?;

            if let rpc::worker::Request::ProcessQuery(query) = request {
                match query.scope {
                    // Pass the query to all known workers.
                    query::Scope::Global => {
                        let workers = manager.get_workers().await?;

                        let mut product = QueryProduct::Empty;

                        // TODO: parallelize.
                        for (id, worker) in workers
                            .into_iter()
                            .filter(|(id, _)| !ctx.worker_observers().contains(id))
                        {
                            match worker
                                .execute(
                                    Signal::from(rpc::worker::Request::ProcessQuery(query.clone()))
                                        .originating_at(Caller::Leader(manager.get_meta().await?)),
                                )
                                .await?
                                .into_payload()
                            {
                                rpc::worker::Response::Query(product_) => {
                                    product.merge(product_)?
                                }
                                _ => unimplemented!(),
                            }
                        }
                        Ok(Signal::new(
                            Response::WorkerProxy(rpc::worker::Response::Query(product)),
                            Some(ctx),
                        ))
                    }
                    query::Scope::Edges(_) => {
                        // use rand::{rngs::SmallRng, seq::IteratorRandom};
                        // let workers = manager.get_workers().await?;
                        // if let Some((worker_id, worker)) = workers
                        //     .iter()
                        //     .filter(|(id, _)| !went_through_workers.contains(id))
                        //     .choose(&mut SmallRng::seed_from_u64(0))
                        // {
                        //     request = rpc::worker::Request::ProcessQuery {
                        //         went_through_leader: true,
                        //         went_through_workers: went_through_workers.clone(),
                        //         query: query.clone(),
                        //     };

                        //     let response = worker.execute(request).await?;
                        //     Ok(Response::WorkerProxy(response))
                        // } else {
                        //     Ok(Response::Empty)
                        // }
                        unimplemented!()
                    }
                    _ => unimplemented!(),
                }
            } else if let rpc::worker::Request::TakeEntity(name, entity) = request {
                // Worker has tasked us with passing the request to take an
                // existing entity to another worker.
                // HACK: choose a random worker.
                let mut workers = manager.get_workers().await?;
                // Don't include the worker that sent the request.
                workers.retain(|w, _| !ctx.went_through_worker(w));
                use rand::{rng, rngs::StdRng, seq::IteratorRandom, SeedableRng};
                if let Some(worker) = workers.values().choose(&mut StdRng::from_os_rng()) {
                    // println!(
                    //     "leader: proxying take_entity request to worker: {}",
                    //     worker.id
                    // );
                    let resp = worker
                        .execute(Signal::new(
                            rpc::worker::Request::TakeEntity(name, entity),
                            Some(ctx.clone()),
                        ))
                        .await?;
                    Ok(Signal::new(Response::WorkerProxy(resp.payload), resp.ctx))
                } else {
                    unimplemented!()
                }
            } else {
                unimplemented!()
            }
        }
        Request::ReorganizeEntities { shuffle } => {
            // manager.reorganize_entities().await?;

            println!("leader: reorganizing entities");
            // TODO: develop further.

            // TODO: parallelize.
            for (id, worker) in &manager.get_workers().await? {
                worker
                    .execute(Signal::new(
                        rpc::worker::Request::MigrateEntities {},
                        ctx.clone(),
                    ))
                    .await?;
            }
            Ok(Signal::new(Response::Empty, ctx))
        }
    }
}

async fn process_step(manager: ManagerExec) -> Result<()> {
    // trace!(
    //     "process step: current clock: {}",
    //     manager.get_clock().await?
    // );

    // First wait for all workers to be ready to go to next step.
    let workers = manager.get_workers().await?;

    if workers.is_empty() {
        return Err(Error::NoAvailableWorkers);
    }

    let mut joins = Vec::new();
    for (worker_id, worker) in &workers {
        joins.push(async move {
            let resp = worker
                .execute(Signal::from(rpc::worker::Request::IsBlocking {
                    wait: true,
                }))
                .await;

            if let Ok(Signal {
                payload: rpc::worker::Response::IsBlocking(false),
                ..
            }) = resp
            {
                return;
            } else {
                error!("{:?}", resp);
                return;
            }
        });
    }
    futures::future::join_all(joins).await;

    // trace!(
    //     "process step: workers ready, current clock: {}",
    //     manager.get_clock().await?
    // );

    // All workers are ready, broadcast the step request.
    let mut joins = Vec::new();
    for (worker_id, worker) in workers.clone() {
        let h = tokio::spawn(async move {
            let resp = worker
                .execute(Signal::from(rpc::worker::Request::Step))
                .await;
            if let Ok(Signal {
                payload: rpc::worker::Response::Step,
                ..
            }) = resp
            {
                return;
            } else {
                error!("{:?}", resp);
                return;
            }
        });
        joins.push(h);
    }
    futures::future::join_all(joins).await;

    // Finally, increment the clock.
    manager.execute(manager::Request::IncrementClock).await?;

    Ok(())
}

async fn initialize_with_scenario(scenario: &str, leader: ManagerExec) -> Result<()> {
    // Generate simulation model for selected scenario.
    let model = leader.get_model().await?;
    initialize(model, leader).await?;
    Ok(())
}

/// Initializes cluster with the provided model.
async fn initialize(model: Model, leader: ManagerExec) -> Result<()> {
    trace!(
        "leader: initializing, connected workers: {:?}",
        leader.get_workers().await?.into_keys()
    );

    // First set the new model on workers across the cluster.
    leader.set_model(model.clone()).await?;

    // Initialize workers.
    let workers = leader.get_workers().await?;
    for (_, worker) in &workers {
        worker
            .execute(Signal::from(rpc::worker::Request::Initialize))
            .await?;
    }

    // Perform additional top-level initialization.

    // Spawn starting entities.
    for entity in &model.entities {
        // HACK: currently distributed randomly.
        use rand::{rng, rngs::StdRng, seq::IteratorRandom, SeedableRng};
        if let Some(worker) = workers.values().choose(&mut StdRng::from_os_rng()) {
            worker
                .execute(Signal::from(rpc::worker::Request::SpawnEntity {
                    name: entity.name.clone(),
                    prefab: entity.prefab.clone(),
                }))
                .await?;
        } else {
            panic!("failed selecting random worker");
        }
    }

    // Spawn global-singleton behaviors.
    for behavior in model.behaviors {
        // Find behaviors that are targetting global singleton instancing.
        if behavior
            .targets
            .contains(&model::behavior::InstancingTarget::GlobalSingleton)
        {
            // Select a worker to spawn the behavior on.
            // TODO: consider certain variables when choosing the worker to
            // spawn the singleton behavior on, such us topology, latencies,
            // available compute, etc.
            // HACK: currently the worker is chosen randomly.
            let worker = leader.get_random_worker().await?;
            worker
                .execute(Signal::from(rpc::worker::Request::SpawnSingletonBehavior(
                    behavior,
                )))
                .await?;
        }
    }

    Ok(())
}

/// Entity distribution policy.
///
/// # Runtime optimization
///
/// Some policies define a more rigid distribution, while others work by
/// actively monitoring the situation across different nodes and transferring
/// entities around as needed.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum DistributionPolicy {
    /// Random distribution.
    Random,
    /// Optimize for processing speed, e.g. using the most capable nodes first.
    MaxSpeed,
    /// Optimize for lowest network traffic, grouping together entities
    /// that tend to cause most inter-machine chatter.
    LowTraffic,
    /// Balanced approach, sane default policy for most cases.
    Balanced,
    /// Focus on similar memory usage across nodes, relative to capability.
    SimilarMemoryUsage,
    /// Focus on similar processor usage across nodes, relative to capability.
    SimilarProcessorUsage,
    /// Spatial distribution using an octree for automatic subdivision of
    /// space to be handled by different workers.
    ///
    /// # Details
    ///
    /// Works with entities that have a `position` component attached. Uses
    /// x, y and z coordinates of an entity and a tree of octant nodes
    /// representing spatial bounds of different workers to assign the entity
    /// to matching worker. In other words, entities are distributed based on
    /// which "worker box" they are currently in.
    // TODO: consider making it into an engine feature, along with position
    // component.
    Spatial,
}
