pub mod config;
pub mod manager;
pub mod part;

mod exec;
mod handle;
mod pov;

pub use config::Config;
pub use exec::{WorkerExec, WorkerRemoteExec};
pub use handle::Handle;

use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tokio_stream::StreamExt;

use fnv::FnvHashMap;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::behavior::BehaviorTarget;
use crate::error::Error;
use crate::executor::{Executor, LocalExec, RemoteExec, Signal};
use crate::leader::{LeaderExec, LeaderRemoteExec};
use crate::model::behavior::BehaviorInner;
use crate::net::{CompositeAddress, ConnectionOrAddress, Encoding};
use crate::query::{self, Trigger};
use crate::rpc::worker::{Request, RequestLocal, Response};
use crate::rpc::Participant;
use crate::server::{ServerExec, ServerId};
use crate::util::{decode, encode};
use crate::worker::manager::ManagerExec;
use crate::{behavior, leader, net, rpc, Model, Query, QueryProduct, Result};

use part::Partition;
use pov::{Leader, LeaderSituation, OtherWorker, Server};

/// Network-unique identifier for a single worker.
pub type WorkerId = Uuid;

#[derive(Debug, Clone)]
pub struct Subscription {
    pub id: Uuid,
    pub triggers: Vec<query::Trigger>,
    pub query: Query,
    pub sender: mpsc::Sender<Result<Signal<Response>>>,
}

pub struct State {
    /// Integer identifier unique across the cluster.
    pub id: WorkerId,

    /// Worker configuration.
    pub config: Config,

    /// List of addresses that can be used to contact this worker.
    pub listeners: Vec<CompositeAddress>,

    /// Simulation model kept up to date with leader.
    pub model: Option<Model>,

    /// Worker-held part of the simulation.
    ///
    /// # Initialization
    ///
    /// Worker can exist within a cluster without being initialized with
    /// a simulation model. For this reason the `part` field is an optional
    /// type.
    pub part: Option<Partition>,

    /// Cluster leader as seen by the worker.
    ///
    /// # Leader loss and re-election
    ///
    /// Leader can run on any of the cluster nodes, it's just another task
    /// to be spawned on the runtime. Nodes have a mechanism for collectively
    /// choosing one of them to spawn a leader.
    // TODO: document leader election mechanism.
    pub leader: LeaderSituation,

    pub servers: FnvHashMap<ServerId, Server>,

    /// Other workers from the same cluster as seen by this worker.
    pub other_workers: FnvHashMap<WorkerId, OtherWorker>,

    pub blocked_watch: (watch::Sender<bool>, watch::Receiver<bool>),
    pub clock_watch: (watch::Sender<usize>, watch::Receiver<usize>),

    pub subscriptions: Vec<Subscription>,
}

/// Creates a new `Worker` task on the current runtime.
///
/// # Usage details
///
/// In a simulation cluster made up of multiple machines, there is at least
/// one `Worker` running on each machine.
///
/// In terms of initialization, `Worker`s can either actively reach out to
/// an already existing cluster to join in, or passively wait for incoming
/// connection from a leader.
///
/// Unless configured otherwise, new `Worker`s can dynamically join into
/// already initialized cluster, introducing on-the-fly changes to the
/// cluster composition.
///
/// # Connection management and topology
///
/// `Worker`s are connected to, and orchestrated by, a single `Leader`.
/// They are also connected to each other. Connections are either direct,
/// or indirect.
///
/// Indirect connections mean messages being routed between cluster members.
/// Leader keeps workers updated about any changes to the cluster membership.
///
/// # Relay-worker
///
/// Worker can be left stateless and serve as a relay, forwarding requests
/// between connected cluster participants.
///
/// Same as a regular stateful worker, relay-worker can be used to back
/// a server that will respond to client queries. Relay-worker-backed servers
/// allow for spreading the load of serving connected clients across more
/// machines, without expanding the core simulation-state-bearing worker base.
///
/// # Local cache
///
/// Worker is able to cache data from responses it gets from other cluster
/// members. Caching behavior can be configured to only allow for up-to-date
/// data to be cached and used for subsequent queries.
pub fn spawn(config: Config, mut cancel: CancellationToken) -> Result<Handle> {
    let cancel = cancel.child_token();

    let (local_ctl_executor, mut local_ctl_stream, _) =
        LocalExec::<Signal<RequestLocal>, Result<Signal<Response>>>::new(20);
    let (local_leader_executor, mut local_leader_stream, _) =
        LocalExec::<Signal<RequestLocal>, Result<Signal<Response>>>::new(20);
    let (local_behavior_executor, mut local_behavior_stream, mut local_behavior_stream_multi) =
        LocalExec::<Signal<Request>, Result<Signal<Response>>>::new(20);
    let (local_server_executor, mut local_server_stream, mut local_server_stream_multi) =
        LocalExec::<Signal<RequestLocal>, Result<Signal<Response>>>::new(20);
    let (net_executor, mut net_stream, _) = LocalExec::new(20);
    net::spawn_listeners(&config.listeners, net_executor.clone(), cancel.clone())?;

    debug!("spawning worker task, listeners: {:?}", config.listeners);

    let clock = watch::channel(0);
    let blocked = tokio::sync::watch::channel(false);
    let id = Uuid::new_v4();
    let mut state = State {
        id,
        // TODO: validate that the listener addresses we're passing here were
        // actually valid and actual listeners were started on those.
        listeners: config.listeners.clone(),
        part: Some(Partition::new(
            local_behavior_executor.clone(),
            config.partition.clone(),
            id,
        )?),
        config,
        leader: LeaderSituation::Never,
        other_workers: FnvHashMap::default(),
        servers: FnvHashMap::default(),
        blocked_watch: blocked,
        clock_watch: clock,
        model: None,
        subscriptions: vec![],
    };

    // Clone the behavior broadcast channel.
    let behavior_broadcast = state.part.as_ref().unwrap().behavior_broadcast.0.clone();

    // Worker state is held by a dedicated manager task.
    let manager = manager::spawn(state, cancel.clone())?;

    let local_leader_executor_c = local_leader_executor.clone();
    let local_behavior_executor_c = local_behavior_executor.clone();

    let cancel_ = cancel.clone();
    let manager_ = manager.clone();
    tokio::spawn(async move {
        loop {
            // debug!("worker loop start");
            tokio::select! {
                Some((sig, s)) = local_ctl_stream.next() => {
                    let worker = manager_.clone();
                    let local_behavior_executor = local_behavior_executor_c.clone();
                    let net_exec = net_executor.clone();
                    let cancel = cancel_.clone();
                    tokio::spawn(async move {
                        debug!("worker: processing controller message");
                        let resp = handle_local_request(sig.payload, sig.ctx, worker, local_behavior_executor, net_exec, None, cancel).await;
                        s.send(resp);
                    });
                },
                Some((sig, s)) = local_server_stream.next() => {
                    debug!("worker: processing message from local server");

                    let worker = manager_.clone();
                    let local_behavior_executor = local_behavior_executor_c.clone();
                    let net_exec = net_executor.clone();
                    let cancel = cancel_.clone();
                    tokio::spawn(async move {
                        // let resp = handle_local_server_request(req, server_id, worker).await;
                        let resp = handle_local_request(sig.payload, sig.ctx, worker, local_behavior_executor, net_exec, None, cancel).await;
                        s.send(resp);
                    });

                },
                Some((sig, s)) = local_server_stream_multi.next() => {
                    debug!("worker: processing message from local server (multi)");

                    let worker = manager_.clone();
                    let local_behavior_executor = local_behavior_executor_c.clone();
                    let net_exec = net_executor.clone();
                    let cancel = cancel_.clone();
                    tokio::spawn(async move {
                        let resp = handle_local_request(sig.payload, sig.ctx, worker, local_behavior_executor, net_exec, Some(s.clone()), cancel).await;
                        s.send(resp).await.unwrap();
                    });

                },
                Some((sig, s)) = local_leader_stream.next() => {
                    use {Response, RequestLocal};
                    // worker receives leader executor channel
                    debug!("worker: processing message from local leader");
                    let worker = manager_.clone();
                    let local_behavior_executor = local_behavior_executor_c.clone();
                    let net_exec = net_executor.clone();
                    let cancel = cancel_.clone();
                    tokio::spawn(async move {
                        let resp = handle_local_request(sig.payload, sig.ctx, worker, local_behavior_executor, net_exec, None, cancel).await;
                        s.send(resp);
                    });

                }
                Some((sig, s)) = local_behavior_stream.next() => {
                    let worker = manager_.clone();
                    let local_behavior_executor = local_behavior_executor_c.clone();
                    let net_exec = net_executor.clone();
                    let cancel = cancel_.clone();
                    tokio::spawn(async move {
                        let resp = handle_request(
                            sig.payload,
                            sig.ctx,
                            worker,
                            local_behavior_executor,
                            net_exec.clone(),
                            None,
                            cancel
                        ).await;
                        s.send(resp);
                    });
                },
                Some((sig, s)) = local_behavior_stream_multi.next() => {
                    let worker = manager_.clone();
                    let local_behavior_executor = local_behavior_executor_c.clone();
                    let net_exec = net_executor.clone();
                    let cancel = cancel_.clone();
                    tokio::spawn(async move {
                        let resp = handle_request(
                            sig.payload,
                            sig.ctx,
                            worker,
                            local_behavior_executor,
                            net_exec.clone(),
                            Some(s.clone()),
                            cancel
                        ).await;
                        s.send(resp);
                    });
                },
                Some(((maybe_con, req), s)) = net_stream.next() => {
                    trace!("worker: processing network message");

                    let worker = manager_.clone();
                    let local_behavior_executor = local_behavior_executor_c.clone();
                    let net_exec = net_executor.clone();
                    let cancel = cancel_.clone();
                    tokio::spawn(async move {
                        let sig: Signal<Request> = match decode(&req, Encoding::Bincode) {
                            Ok(r) => r,
                            Err(e) => {
                                error!("failed decoding request (bincode)");
                                return;
                            }
                        };
                        let resp = handle_net_request(sig.payload, sig.ctx, worker.clone(), maybe_con, local_behavior_executor, net_exec, cancel).await;
                        s.send(encode(resp, Encoding::Bincode).unwrap()).unwrap();
                    });
                },
                _ = cancel_.cancelled() => {
                    debug!("worker[{}]: shutting down", id);
                    break;
                }
            }
        }
    });

    let cancel = cancel.clone();
    let manager = manager.clone();
    let local_ctl_executor_ = local_ctl_executor.clone();
    tokio::spawn(async move {
        let worker_id = manager.get_id().await.unwrap();
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(1000)) => {
                    let leader = manager.get_leader().await.unwrap();
                    match leader {
                        LeaderSituation::Election | LeaderSituation::OrphanedWaiting | LeaderSituation::Never => continue,
                        _ => (),
                    }
                    // println!("worker: manager: leader connected, checking if alive");
                    if let LeaderSituation::Connected(leader) = &leader {
                        if let Ok(_) = leader.execute(Signal::from(rpc::leader::Request::Status)).await {
                            continue;
                        } else {
                            let old_leader_addrs = leader.listeners.clone();

                            // Leader not reachable.
                            warn!("worker[{}]: manager: leader connection lost", worker_id);

                            if manager.get_other_workers().await.unwrap().is_empty() {
                                // If the worker doesn't have any remote workers in
                                // view it's now orphaned.
                                warn!(
                                    "worker[{}]: worker orphaned, no remote workers known",
                                    worker_id
                                );
                                manager
                                    .set_leader(LeaderSituation::OrphanedWaiting)
                                    .await
                                    .unwrap();

                                let cancel = cancel.clone();
                                let manager = manager.clone();
                                let local_ctl_executor = local_ctl_executor_.clone();
                                tokio::spawn(async move {
                                    // It may be that, even though it doesn't
                                    // have other workers in view, others might
                                    // still contact it.
                                    //
                                    // Wait for some amount of time to allow
                                    // other workers to reach out.
                                    //
                                    // In the meantime also try reconnecting,
                                    // maybe  the leader restarts in time and
                                    // it's possible to re-establish the
                                    // connection.
                                    tokio::time::sleep(Duration::from_secs(4)).await;

                                    if let Some(old_leader_addr) = old_leader_addrs.first().cloned() {
                                        println!("worker: trying to reconnect to previously connected leader: {}", old_leader_addr);
                                        if let Ok(sig) = local_ctl_executor.execute(
                                            Signal::from(RequestLocal::Request(Request::ConnectToLeader(old_leader_addr))))
                                                .await.unwrap() {
                                            let resp = sig.into_payload();
                                            if let Response::Empty = resp {
                                                println!("worker: successfully reconnected to previously connected leader");
                                                return;
                                            }
                                        }
                                    }

                                    // Check if the leader situation has changed.
                                    match manager.get_leader().await.unwrap() {
                                        LeaderSituation::Election | LeaderSituation::Connected(_) => return,
                                        _ => {
                                            match manager.get_config().await {
                                                Ok(Config { orphan_fork: true, .. }) => {
                                                    // Attempt to spawn a new leader
                                                    // without an election.
                                                    // TODO: spawning the leader should probably go through
                                                    // the local node, if available. This way we would
                                                    // maintain a handle to the leader.
                                                    let leader_addr = net::get_available_address().expect("unable to find exposable addr");
                                                    let mut leader_address = CompositeAddress::default();
                                                    leader_address.address = net::Address::Net(leader_addr);
                                                    trace!("worker orphan: spawning leader");
                                                    leader::spawn(leader::Config { listeners: vec![leader_address], autostep: Some(Duration::from_millis(10)), ..Default::default() }, CancellationToken::new());

                                                    trace!("worker orphan: connecting to spawned leader");
                                                    // HACK: the way we build the leader address is
                                                    // suboptimal.
                                                    let req =
                                                        rpc::worker::Request::ConnectToLeader(format!("127.0.0.1:{}", leader_addr.port()).parse().unwrap()).into_local();
                                                    local_ctl_executor.execute(Signal::from(req)).await.unwrap().unwrap();
                                                },
                                                _ => {
                                                    // Shut down the whole worker.
                                                    debug!(
                                                        "worker[{}]: shutting down due to being orphaned for too long",
                                                        worker_id
                                                    );
                                                    cancel.cancel();
                                                }
                                            }


                                        }
                                    }

                                });
                            } else {
                                // Otherwise the workers can attempt to elect
                                // a new leader.

                                // Update the leader situation for this worker.
                                manager.set_leader(LeaderSituation::Election).await.unwrap();

                                // TODO: call the local node. Ask for it's
                                // status/stats.

                                // TODO: consult the stats with the other
                                // workers. Collectively select a single
                                // worker from amongst themselves that will
                                // call upon it's local node to spawn a new
                                // leader.
                                for (id, other_worker) in &manager.get_other_workers().await.unwrap() {
                                    println!(
                                        "worker[{}]: election: known remote worker: {}",
                                        worker_id, id
                                    );

                                    println!("worker[{}]: sending election request to {}", worker_id, id);
                                    if let Ok(resp) =
                                        other_worker.execute(Request::Election { sender_id: worker_id }).await
                                    {
                                        if let Response::Empty = resp {
                                            //
                                        } else {
                                            unimplemented!()
                                        }
                                    }
                                }

                                // TODO: during leader spawning we need to pass
                                // it the latent view of the cluster, meaning
                                // all the workers' connect info.
                            }
                        }
                    }
                }
                _ = cancel.cancelled() => {
                    break;
                }
            }
        }
    });

    Ok(Handle {
        server_exec: local_server_executor,
        ctl: local_ctl_executor,
        leader_exec: local_leader_executor,
        behavior_exec: local_behavior_executor,
        behavior_broadcast,
    })
}

async fn handle_local_request(
    req: RequestLocal,
    ctx: Option<rpc::Context>,
    manager: ManagerExec,
    behavior_exec: LocalExec<Signal<Request>, Result<Signal<Response>>>,
    net_exec: LocalExec<(ConnectionOrAddress, Vec<u8>), Vec<u8>>,
    multi: Option<mpsc::Sender<Result<Signal<Response>>>>,
    cancel: CancellationToken,
) -> Result<Signal<Response>> {
    match req {
        RequestLocal::ConnectToLeader(_worker_leader, leader_worker) => {
            let my_id = manager.get_id().await?;

            _worker_leader
                .execute(Signal::new(
                    rpc::leader::RequestLocal::ConnectAndRegisterWorker(leader_worker),
                    Some(
                        ctx.clone()
                            .unwrap_or(rpc::Context::new(Participant::Worker(my_id).into())),
                    ),
                ))
                .await??;

            let worker_leader = Leader {
                exec: LeaderExec::Local(_worker_leader),
                worker_id: my_id,
                listeners: vec![],
            };
            manager
                .set_leader(LeaderSituation::Connected(worker_leader))
                .await?;

            Ok(Signal::new(
                Response::ConnectToLeader { worker_id: my_id },
                ctx,
            ))
        }
        RequestLocal::IntroduceLeader(exec) => {
            log::debug!("worker connecting to leader...");

            let my_id = manager.get_id().await?;
            let exec = LeaderExec::Local(exec);

            match manager.get_leader().await? {
                LeaderSituation::Connected(_) => {
                    log::debug!("worker already aware of a leader");
                }
                _ => {
                    let leader = LeaderSituation::Connected(Leader {
                        exec,
                        worker_id: my_id,
                        listeners: vec![],
                    });
                    manager.set_leader(leader).await;
                }
            };

            log::debug!("worker successfuly connected to leader");

            Ok(Signal::new(Response::Empty, ctx))
        }
        RequestLocal::ConnectToServer(worker_server, server_worker) => {
            let sig = worker_server
                .execute(Signal::new(
                    rpc::server::RequestLocal::IntroduceWorker(server_worker.clone()),
                    ctx.clone(),
                ))
                .await??;
            if let rpc::server::Response::IntroduceWorker(server_id) = sig.into_payload() {
                let server = Server {
                    worker_id: manager.get_id().await?,
                    server_id,
                    exec: ServerExec::Local(worker_server),
                };
                manager.insert_server(server_id, server).await?;
                Ok(Signal::new(Response::Empty, ctx))
            } else {
                Err(Error::UnexpectedResponse("".to_string()))
            }
        }
        RequestLocal::IntroduceServer(server_id, exec) => {
            let server = Server {
                worker_id: manager.get_id().await?,
                server_id,
                exec: ServerExec::Local(exec),
            };

            manager.insert_server(server_id, server).await?;

            Ok(Signal::new(Response::Register { server_id }, ctx))
        }
        RequestLocal::ConnectToWorker() => todo!(),
        RequestLocal::AddBehavior(target, handle) => {
            manager.add_behavior(target, handle).await?;
            Ok(Signal::new(Response::Empty, ctx))
        }
        RequestLocal::Request(req) => {
            handle_request(req, ctx, manager, behavior_exec, net_exec, multi, cancel).await
        }
        RequestLocal::Shutdown => {
            manager.shutdown().await?;
            Ok(Signal::new(Response::Empty, ctx))
        }
    }
}

/// Network-oriented request handler, exposing some additional network-specific
/// context on the incoming request.
async fn handle_net_request(
    req: rpc::worker::Request,
    ctx: Option<rpc::Context>,
    manager: ManagerExec,
    maybe_con: ConnectionOrAddress,
    behavior_exec: LocalExec<Signal<Request>, Result<Signal<Response>>>,
    net_exec: LocalExec<(ConnectionOrAddress, Vec<u8>), Vec<u8>>,
    cancel: CancellationToken,
) -> Result<Signal<Response>> {
    match req {
        rpc::worker::Request::IntroduceWorker(id) => {
            match maybe_con {
                ConnectionOrAddress::Connection(con) => {
                    // re-use the connection if the request was transported
                    // via quic
                    manager
                        .add_other_worker(OtherWorker {
                            local_id: manager.get_id().await?,
                            id,
                            exec: WorkerExec::Remote(RemoteExec::new(con)),
                        })
                        .await?;
                }
                ConnectionOrAddress::Address(addr) => {
                    // as a fallback use the address of the caller known here
                    unimplemented!()
                }
            }

            let my_id = manager.get_id().await?;

            Ok(Signal::new(Response::IntroduceWorker(my_id), ctx))
        }
        rpc::worker::Request::IntroduceLeader {
            listeners,
            cluster_view,
        } => {
            let my_id = manager.get_id().await?;

            let remote_exec = if let ConnectionOrAddress::Connection(con) = maybe_con {
                RemoteExec::new(con)
            } else {
                unimplemented!()
            };
            let exec = LeaderExec::Remote(remote_exec);

            println!("worker: introduce_leader: cluster_view {:?}", cluster_view);
            for (id, worker) in cluster_view {
                if let Some(worker_addr) = worker.first() {
                    // Initiate a new connection to the remote worker.
                    let connection =
                        net::quic::make_connection(worker_addr.address.clone().try_into().unwrap())
                            .await
                            .map_err(|e| Error::NetworkError(e.to_string()))?;
                    let exec = WorkerExec::Remote(RemoteExec::new(connection));
                    manager
                        .add_other_worker(OtherWorker {
                            local_id: my_id,
                            id,
                            exec,
                        })
                        .await?;
                    println!("added remote worker to worker");
                }
            }

            manager
                .set_leader(LeaderSituation::Connected(Leader {
                    exec,
                    worker_id: my_id,
                    listeners,
                }))
                .await?;
            Ok(Signal::new(
                Response::IntroduceLeader { worker_id: my_id },
                ctx,
            ))
        }
        rpc::worker::Request::Election { sender_id } => {
            // Add the worker that sent the request to our view.
            match maybe_con {
                ConnectionOrAddress::Connection(con) => {
                    // Re-use the connection if the request was transported
                    // via quic.
                    manager
                        .add_other_worker(OtherWorker {
                            local_id: manager.get_id().await?,
                            id: sender_id,
                            exec: WorkerExec::Remote(RemoteExec::new(con)),
                        })
                        .await?;
                }
                ConnectionOrAddress::Address(addr) => {
                    // as a fallback use the address of the caller known here
                    unimplemented!()
                }
            }

            let my_id = manager.get_id().await?;
            println!(
                "worker[{}]: got election request from other worker: {}",
                my_id, sender_id
            );

            // Step-up from an orphaned status to in-progress election, as the
            // worker is now connected to at least one other worker from the
            // previously established cluster.
            manager.set_leader(LeaderSituation::Election).await?;

            Ok(Signal::new(Response::Empty, ctx))
        }
        _ => handle_request(req, ctx, manager, behavior_exec, net_exec, None, cancel).await,
    }
}

async fn handle_request(
    req: rpc::worker::Request,
    mut ctx: Option<rpc::Context>,
    manager: ManagerExec,
    behavior_exec: LocalExec<Signal<Request>, Result<Signal<Response>>>,
    net_exec: LocalExec<(ConnectionOrAddress, Vec<u8>), Vec<u8>>,
    multi: Option<mpsc::Sender<Result<Signal<Response>>>>,
    cancel: CancellationToken,
) -> Result<Signal<Response>> {
    use rpc::worker::Request;

    debug!("worker: processing request: {req}");

    match req {
        Request::PullConfig(config) => {
            manager.pull_config(config).await?;
            Ok(Signal::new(Response::Empty, ctx))
        }
        Request::ConnectToLeader(address) => {
            let bind = SocketAddr::from_str("0.0.0.0:0").unwrap();
            // let endpoint = quinn::Endpoint::client(a).unwrap();
            trace!("worker: connecting to leader: {:?}", address);
            let endpoint = net::quic::make_client_endpoint_insecure(bind).unwrap();
            let connection = endpoint
                .connect(address.clone().address.try_into().unwrap(), "any")?
                .await?;
            trace!("worker: connected to leader");

            let remote_exec = LeaderRemoteExec::new(connection.clone());
            trace!("worker: remote executor created");

            let my_id = manager.get_id().await?;
            println!("introducing worker");
            let req = rpc::leader::Request::IntroduceWorker(my_id);
            let sig = remote_exec
                .execute(Signal::from(req).originating_at(Participant::Worker(my_id).into()))
                .await??;
            println!("done introducing worker");
            trace!("worker: sent introduction: got response: {sig:?}");

            // Leader responds with the model it has.
            let resp = sig.into_payload();
            if let rpc::leader::Response::Model(model) = resp {
                // Store the leader executor so that we can send data to the leader
                manager
                    .set_leader(LeaderSituation::Connected(Leader {
                        exec: LeaderExec::Remote(remote_exec),
                        worker_id: my_id,
                        listeners: vec![address],
                    }))
                    .await?;

                // If the leader was already initialized with a model, then we get
                // that model with the response
                manager.set_model(model).await?;

                manager.initialize(behavior_exec).await?;
            }
            // Otherwise we proceed without a model, hoping that leader
            // provides us with a model later
            else if let rpc::leader::Response::Empty = resp {
                // Store the leader executor so that we can send data to the leader
                manager
                    .set_leader(LeaderSituation::Connected(Leader {
                        exec: LeaderExec::Remote(remote_exec),
                        worker_id: my_id,
                        listeners: vec![address],
                    }))
                    .await?;
            } else {
                unimplemented!("unexpected response: {:?}", resp);
            }

            tokio::spawn(async move {
                if let Err(e) =
                    net::quic::handle_connection(net_exec.clone(), connection, cancel).await
                {
                    error!("connection failed: {reason}", reason = e.to_string())
                }
            });

            Ok(Signal::new(Response::Empty, ctx))
        }
        Request::Ping(bytes) => Ok(Signal::new(Response::Ping(bytes), ctx)),
        Request::MemorySize => {
            let size = manager.memory_size().await?;
            trace!("worker mem size: {}", size);
            Ok(Signal::new(Response::MemorySize(size), ctx))
        }
        Request::IsBlocking { wait } => {
            trace!("received IsBlocking request: wait: {wait}");
            let mut is_blocked = manager.get_blocked_watch().await?.clone();
            if *is_blocked.borrow() == true {
                trace!("worker is blocked, waiting to unblock");
                loop {
                    if *is_blocked.borrow() == false {
                        debug!("worker unblocked");
                        return Ok(Signal::new(Response::IsBlocking(false), ctx));
                    } else {
                        is_blocked.changed().await;
                        // println!("last thing that worker did?");
                    }
                }
            } else {
                trace!("worker is not blocked");
                Ok(Signal::new(Response::IsBlocking(false), ctx))
            }
        }
        Request::Step => {
            let current_clock = manager.get_clock_watch().await?.borrow().clone();

            let new_clock = current_clock + 1;
            manager.set_clock_watch(new_clock).await?;
            trace!(">> did set clock watch clock + 1");
            for (server_id, server) in manager.get_servers().await? {
                server
                    .execute(Signal::new(
                        rpc::server::Request::ClockChangedTo(new_clock),
                        ctx.clone(),
                    ))
                    .await;
            }
            trace!("sent clockchangedto to all servers");

            let event = "step".to_owned();

            let broadcast = manager.get_unsynced_behavior_tx().await?;
            broadcast
                .send(rpc::behavior::Request::Event(event.clone()))
                .inspect_err(|e| {
                    warn!("worker: failed propagating event trigger to unsynced behaviors: {e}")
                });

            let mut set = JoinSet::new();
            for handle in manager
                .get_synced_behavior_handles()
                .await?
                .into_values()
                .flatten()
            {
                // Only send the event to the behavior if it "subscribed" the
                // event with a proper trigger.
                if handle.triggers.contains(&event) {
                    // TODO: figure out how to process responses here
                    let event = event.clone();
                    let mut ctx = ctx.clone();
                    set.spawn(async move {
                        handle
                            .execute(Signal::new(
                                rpc::behavior::Request::Event(event),
                                ctx.clone(),
                            ))
                            .await
                    });
                    // .await
                    // .inspect_err(|e| {
                    //     warn!("failed on event request on synced behavior {e}")
                    // })??;
                }
            }

            let _ = set.join_all().await;

            #[cfg(feature = "machine")]
            for machine in manager.get_machine_handles().await? {
                // Only send the event to the behavior if it "subscribed" the
                // event with a proper trigger.
                if machine.behavior.triggers.contains(&event) {
                    let _ = machine
                        // .execute(rpc::machine::Request::Step)
                        .behavior
                        .execute(Signal::new(
                            rpc::behavior::Request::Event(event.clone()),
                            ctx.clone(),
                        ))
                        .await
                        .inspect_err(|e| {
                            warn!("failed on event request on (synced) machine {e}")
                        })??;
                }
            }

            // Process step-trigered subscriptions.
            let subs = manager.get_subscriptions().await?;
            for sub in subs {
                if !sub.triggers.contains(&Trigger::StepEvent) {
                    continue;
                }
                let product = manager.process_query(sub.query).await?;
                sub.sender
                    .send(Ok(Signal::from(Response::Query(product))))
                    .await;
            }

            trace!("worker processed step");
            Ok(Signal::new(Response::Step, ctx))
        }
        Request::Initialize => {
            trace!(">>> worker: initializing");

            manager.set_clock_watch(0).await?;

            manager.initialize(behavior_exec.clone()).await?;

            Ok(Signal::from(Response::Empty))
        }
        Request::GetModel => {
            let leader = manager.get_leader().await?;
            if let LeaderSituation::Connected(leader) = leader {
                match leader
                    .execute(Signal::new(rpc::leader::Request::Model, ctx.clone()))
                    .await?
                    .into_payload()
                {
                    rpc::leader::Response::Model(model) => {
                        Ok(Signal::new(Response::GetModel(model), ctx))
                    }
                    response => Err(Error::UnexpectedResponse(response.to_string())),
                }
            } else {
                Err(Error::LeaderNotConnected(format!("{:?}", leader)))
            }
        }
        Request::ReplaceModel(model) => {
            // Propagate request to leader.
            let leader = manager.get_leader().await?;
            if let LeaderSituation::Connected(leader) = leader {
                let sig = leader
                    .execute(Signal::new(rpc::leader::Request::ReplaceModel(model), ctx))
                    .await?;
                Ok(Signal::new(Response::PullProject, sig.ctx))
            } else {
                Err(Error::LeaderNotConnected(format!("{:?}", leader)))
            }
        }
        Request::MergeModel(model) => {
            let leader = manager.get_leader().await?;
            if let LeaderSituation::Connected(leader) = leader {
                let sig = leader
                    .execute(Signal::new(rpc::leader::Request::MergeModel(model), ctx))
                    .await?;
                Ok(Signal::new(Response::Empty, sig.ctx))
            } else {
                Err(Error::LeaderNotConnected(format!("{:?}", leader)))
            }
        }
        Request::SetModel(model) => {
            if let Ok(current_model) = manager.get_model().await {
                if manager.get_config().await?.behaviors_follow_model_changes {
                    // Perform a diff on the model.
                    //
                    // We're especially interested in finding behaviors that need to
                    // be restarted due to changes with the new model.

                    // TODO: move this diffing logic to a separate function.

                    let mut changed_behaviors = vec![];

                    let current_model = manager.get_model().await?;
                    for current_behavior in current_model.behaviors {
                        if let Some(new_behavior) = model
                            .behaviors
                            .iter()
                            .find(|b| b.name == current_behavior.name)
                        {
                            if &current_behavior != new_behavior {
                                // println!(
                                //     "worker: set_model: found changed behavior by name: {}",
                                //     current_behavior.name
                                // );
                                changed_behaviors.push(current_behavior.name);
                            } else {
                                // No changes.
                            }
                        } else {
                            // New model doesn't have the named behavior that existed
                            // with the old model. (deleted or renamed)
                            // TODO: decide what to do here. Perhaps we should have
                            // a config value specifying if there should be eager
                            // removal of "outdated" behavior tasks. This wouldn't
                            // play well with spawning arbitrary behaviors though,
                            // e.g. those created from closures on the `SimHandle`
                            // level.
                        }
                    }

                    // Trigger restarting selected behaviors.
                    let handles = manager.get_synced_behavior_handles().await?;
                    let handles = handles
                        .iter()
                        .map(|(_, list)| list)
                        .flatten()
                        .collect::<Vec<_>>();
                    for name in changed_behaviors {
                        for handle in handles.iter().filter(|handle| handle.name == name) {
                            // println!("shutting down old behavior");
                            handle
                                .execute(Signal::new(rpc::behavior::Request::Shutdown, None))
                                .await??;

                            if let Some(new_behavior) =
                                model.behaviors.iter().find(|b| b.name == name)
                            {
                                // TODO: encapsulate spawning behaviors based
                                // on `BehaviorInner` and put it in the behavior
                                // module.
                                let new_handle = match &new_behavior.inner {
                                    #[cfg(feature = "behavior_lua")]
                                    BehaviorInner::Lua { synced, script } => behavior::lua::spawn(
                                        name.clone(),
                                        script.clone(),
                                        new_behavior.triggers.clone(),
                                        BehaviorTarget::Worker,
                                        behavior_exec.clone(),
                                    )?,
                                    _ => unimplemented!(),
                                };

                                manager
                                    .add_behavior(BehaviorTarget::Worker, new_handle)
                                    .await?;
                            }
                        }
                    }
                }

                // Set the new model as current.
                manager.set_model(model).await?;
            } else {
                // Setting the model where there previously was none.
                manager.set_model(model).await?;
            }

            Ok(Signal::new(Response::Empty, ctx))
        }
        Request::Clock => {
            let leader = manager.get_leader().await?;
            if let LeaderSituation::Connected(leader) = leader {
                trace!("worker_id: {:?}", leader.worker_id);
                let clock = match leader
                    .execute(Signal::from(rpc::leader::Request::Clock))
                    .await?
                    .into_payload()
                {
                    rpc::leader::Response::Clock(clock) => clock,
                    rpc::leader::Response::Empty => panic!("got unexpected empty response"),
                    _ => panic!("worker failed getting clock value from leader"),
                };
                Ok(Signal::new(Response::Clock(clock), ctx))
            } else {
                Err(Error::LeaderNotConnected(format!("{:?}", leader)))
            }
        }
        Request::Subscribe(trigger, query) => {
            if let Some(multi) = multi {
                let id = manager.subscribe(trigger, query, multi).await?;
                Ok(Signal::from(Response::Subscribe(id)))
            } else {
                Err(Error::Other(
                    "handler wasn't provided with a `multi` sender".to_string(),
                ))
            }
        }
        Request::Unsubscribe(id) => {
            manager.unsubscribe(id).await?;
            Ok(Signal::new(Response::Empty, ctx))
        }
        Request::ProcessQuery(query) => {
            let mut ctx = ctx.ok_or(Error::ContextRequired(
                "Worker cannot process query without signal context".to_string(),
            ))?;
            let product = process_query(query, manager, &mut ctx).await?;
            trace!("worker: query: product: {:?}", product);
            Ok(Signal::new(Response::Query(product), Some(ctx)))
        }
        Request::SetBlocking(blocking) => {
            manager.set_blocked_watch(blocking).await?;
            trace!("set worker blocked watch to {}", blocking);
            Ok(Signal::new(Response::Empty, ctx))
        }
        Request::TakeEntity(name, mut entity) => {
            println!(
                "worker[{}]: take entity: {}, {:?}, signal ctx: {:?}",
                manager.get_id().await?,
                name,
                entity,
                ctx
            );

            entity.meta.last_moved = Some(Utc::now().timestamp() as u32);

            // Store the incoming entity as our own.
            manager.add_entity(name, entity).await?;

            // TODO: entity durability story.

            Ok(Signal::new(Response::Empty, ctx))
        }
        Request::GetEntity(name) => {
            let entity = manager.get_entity(name).await?;
            Ok(Signal::new(Response::Entity(entity), ctx))
        }
        Request::EntityList => {
            let entities = manager.get_entities().await?;
            Ok(Signal::new(Response::EntityList(entities), ctx))
        }
        Request::NewRequirements {
            ram_mb,
            disk_mb,
            transfer_mb,
        } => todo!(),
        Request::Status => {
            // TODO: contact manager
            Ok(Signal::new(
                Response::Status {
                    id: manager.get_id().await?,
                    // TODO get status information from manager.
                    uptime: 1,
                    worker_count: 1,
                },
                ctx,
            ))
        }
        Request::Invoke { events, global } => {
            // Global invocation requires broadcasting the request accross the
            // cluster to all the workers.
            if global {
                // ctx.unwrap().worker_hop_count()
                // manager.broadcast()
            }
            // Local invocation can happen right away.
            else if !global {
            }

            // Broadcast events to unsynced behaviors
            let tx = manager.get_unsynced_behavior_tx().await?;
            for event in &events {
                tx.send(rpc::behavior::Request::Event(event.clone()))
                    .inspect_err(|_| {
                        // warn!("attempted broadcasting to unsynced behaviors, but there are none")
                    });
            }

            // println!("triggering events: {events:?}");

            let behavior_handles = manager
                .get_synced_behavior_handles()
                .await?
                .into_values()
                .flatten()
                .collect::<Vec<_>>();
            #[cfg(feature = "machine")]
            let machine_handles = manager
                .get_machine_handles()
                .await?
                .into_iter()
                .map(|h| h.behavior)
                .collect::<Vec<_>>();
            #[cfg(feature = "machine")]
            let behavior_handles = behavior_handles
                .into_iter()
                .chain(machine_handles)
                .collect::<Vec<_>>();

            for handle in behavior_handles {
                for event in &events {
                    if handle.triggers.contains(&event) {
                        // println!("triggering {event} for behavior handle");
                        // TODO: take into consideration behavior-level triggers' filter
                        let resp = handle
                            .execute(Signal::new(
                                rpc::behavior::Request::Event(event.clone()),
                                ctx.clone(),
                            ))
                            .await;
                        // println!("resp: {resp:?}");
                    }
                }
            }

            // Process event-trigered subscriptions.
            for event in events {
                let subs = manager.get_subscriptions().await?;
                for sub in subs {
                    println!("event: {}, sub: {:?}", event, sub);
                    for trigger in &sub.triggers {
                        if let Trigger::Event(event) = trigger {
                            continue;
                        }
                    }

                    let mut ctx_ = ctx.clone().ok_or(Error::ContextRequired(format!("")))?;
                    let manager_ = manager.clone();
                    let query = sub.query.clone();

                    let (product, ctx) = tokio::spawn(async move {
                        let mut ctx_ = ctx_;
                        let product = process_query(query, manager_, &mut ctx_).await?;
                        Result::Ok((product, ctx_))
                    })
                    .await
                    .unwrap()?;

                    sub.sender
                        .send(Ok(Signal::new(Response::Query(product), Some(ctx))))
                        .await;
                }
            }

            Ok(Signal::new(Response::Empty, ctx))
        }
        Request::SpawnEntity { name, prefab } => {
            manager.spawn_entity(name, prefab).await?;
            Ok(Signal::new(Response::Empty, ctx))
        }
        Request::DespawnEntity { name } => {
            manager.remove_entity(name).await?;
            Ok(Signal::new(Response::Empty, ctx))
        }
        Request::ConnectToWorker(address) => {
            // Received a request to connect to remote worker at given address.

            // Initiate a new connection to the remote worker.
            let connection = net::quic::make_connection(address.address.try_into().unwrap())
                .await
                .map_err(|e| Error::NetworkError(e.to_string()))?;
            let remote_exec = WorkerRemoteExec::new(connection);

            // Send the introductory message.
            let my_id = manager.get_id().await?;
            println!("myid: {}", my_id);
            let sig = remote_exec
                .execute(
                    Signal::from(Request::IntroduceWorker(my_id))
                        .originating_at(Participant::Worker(my_id).into()),
                )
                .await??;

            // Parse the response and store the remote worker handle.
            let worker_id = match sig.into_payload() {
                Response::IntroduceWorker(id) => id,
                _ => unimplemented!(),
            };
            let other_worker = OtherWorker {
                local_id: my_id,
                id: worker_id,
                exec: WorkerExec::Remote(remote_exec),
            };
            manager.add_other_worker(other_worker).await?;

            // At this point we have two or more workers connected, which could
            // constitute a cluster, but we don't know if there's a leader.

            // Trigger a leader check.
            match manager.get_leader().await {
                Err(Error::LeaderNotSelected(_)) => {
                    // there's no leader found across all workers

                    // initiate leader election

                    // broadcast
                    let workers = manager.get_other_workers().await?;

                    unimplemented!();

                    // let _ = manager.elect_leader().await?;
                }
                Ok(leader) => {
                    // leader was found, cluster is functional
                    trace!("leader was found");
                }
                _ => (),
            }

            Ok(Signal::new(Response::Empty, ctx))
        }
        Request::GetLeader => {
            let leader = manager.get_leader().await?;

            if let LeaderSituation::Connected(leader) = leader {
                let leader = match leader.exec {
                    LeaderExec::Remote(remote) => Some(remote.remote_address()),
                    // LeaderExec::Remote(remote_exec) => remote_exec.local_ip(),
                    LeaderExec::Local(local_exec) => {
                        // TODO: ask leader for their network listener addresses
                        // panic!("unable to send back leader addr as it's stored as local")
                        None
                    }
                };

                Ok(Signal::new(Response::GetLeader(leader), ctx))
            } else {
                Ok(Signal::new(Response::GetLeader(None), ctx))
            }
        }
        Request::GetListeners => Ok(Signal::new(
            Response::GetListeners(manager.get_listeners().await?),
            ctx,
        )),
        Request::SetVar(address, var) => {
            manager.set_vars(vec![(address, var)]).await?;
            Ok(Signal::new(Response::Empty, ctx))
        }
        Request::SetVars(pairs) => {
            manager.set_vars(pairs).await?;
            Ok(Signal::new(Response::Empty, ctx))
        }
        Request::GetVar(address) => Ok(Signal::new(
            Response::GetVar(manager.get_var(address).await?),
            ctx,
        )),
        Request::SpawnSingletonBehavior(behavior) => {
            // FLOW: Leader called the worker to spawn a behavior task that will be
            // managed by the leader.

            // todo!();
            println!("TODO: spawning singleton behavior");

            // behavior::spawn_non_entity_bound(model, part, worker_exec)?;
            Ok(Signal::new(Response::Empty, ctx))
        }
        Request::MigrateEntities {} => {
            let entities = manager.get_entities().await?;
            let config = manager.get_config().await?;

            // For each entity, serialize and send over to another worker.
            for entity_name in entities {
                let resp = manager
                    .execute(manager::Request::GetEntity(entity_name.clone()))
                    .await??;
                if let manager::Response::Entity(entity) = resp {
                    // Prevent a single entity getting moved multiple times
                    // during a single migration process.
                    if let Some(cooldown_secs) = config.entity_migration_cooldown_secs {
                        println!("last moved: {:?}", entity.meta.last_moved);
                        if let Some(last_moved) = entity.meta.last_moved {
                            if Utc::now().timestamp() as u32 - last_moved <= cooldown_secs as u32 {
                                println!("cooldown not ready");
                                continue;
                            }
                        }
                    }
                    let workers = manager.get_other_workers().await?;
                    if workers.is_empty() {
                        // If no remote workers are connected, send through the
                        // leader. The leader will decide where to migrate the
                        // entity based on it's own process.
                        let leader_sit = manager.get_leader().await?;
                        if let LeaderSituation::Connected(leader) = leader_sit {
                            leader
                                .execute(Signal::new(
                                    rpc::leader::Request::WorkerProxy(Request::TakeEntity(
                                        entity_name.clone(),
                                        entity,
                                    )),
                                    ctx.clone().or(Some(rpc::Context::new(
                                        Participant::Worker(manager.get_id().await?).into(),
                                    ))),
                                ))
                                .await?;
                        }
                    } else {
                        // Choose a particular worker to send the entity to.
                        // HACK: currently we just choose a random worker.
                        use rand::{rng, rngs::StdRng, seq::IteratorRandom, SeedableRng};
                        if let Some(worker) = workers.values().choose(&mut StdRng::from_os_rng()) {
                            worker
                                .execute(Request::TakeEntity(entity_name.clone(), entity))
                                .await?;
                        }
                    }

                    // Entity migration issued, now remove the entity from the
                    // worker.
                    // TODO: consider first double-checking that it was
                    // succesfully migrated.
                    manager.remove_entity(entity_name).await?;
                } else {
                    warn!(
                        "Entity was listed as existing on the worker but
                        we failed fetching it."
                    );
                }
            }
            Ok(Signal::new(Response::Empty, ctx))
        }
        Request::Authorize { token } => {
            todo!()
        }
        Request::Snapshot => {
            //
            Ok(Signal::new(Response::Empty, ctx))
        }
        #[cfg(feature = "machine")]
        Request::MachineLogic { name } => {
            let leader = manager.get_leader().await?;

            let model = if let LeaderSituation::Connected(leader) = leader {
                match leader
                    .execute(Signal::new(rpc::leader::Request::Model, ctx.clone()))
                    .await?
                {
                    Signal {
                        payload: rpc::leader::Response::Model(model),
                        ..
                    } => model,
                    _ => panic!(),
                }
            } else {
                unimplemented!()
            };

            let behavior = model
                .behaviors
                .into_iter()
                .find(|bhvr| bhvr.name == name)
                .unwrap();

            #[allow(irrefutable_let_patterns)]
            if let BehaviorInner::Machine { script, logic } = behavior.inner {
                Ok(Signal::new(Response::MachineLogic(logic), ctx))
            } else {
                unimplemented!()
            }
        }
        Request::IntroduceWorker(_) => unreachable!(),
        Request::IntroduceLeader { .. } => unreachable!(),
        Request::Election { .. } => unreachable!(),
    }
}

/// Broadcasts a request accross all cluster participants.
async fn broadcast(query: Query, manager: ManagerExec, mut ctx: &mut rpc::Context) -> Result<()> {
    Ok(())
}

async fn process_query(
    query: Query,
    manager: ManagerExec,
    mut ctx: &mut rpc::Context,
) -> Result<QueryProduct> {
    let my_id = manager.get_id().await?;
    let mut product = QueryProduct::Empty;
    match query.scope {
        query::Scope::Global => {
            // println!("worker[{my_id}]: processing global query");

            // Broadcast query accross the cluster

            let mut workers = manager.get_other_workers().await?;
            // println!(
            //     "local worker: visible workers: {:?}",
            //     workers.iter().map(|(id, _)| id).collect::<Vec<&Uuid>>()
            // );

            // It can be that no remote workers are visible to this
            // worker. Cluster creation is allowed with only the leader
            // being accessible over the network.
            //
            // In such case the worker must use the leader as proxy
            // towards the rest of the cluster.
            //
            // TODO: put some of these peering rules into worker config.
            if workers.is_empty() && !ctx.went_through_leader() {
                // Add the current worker to the list so that the
                // request doesn't circle back through the leader to
                // this worker.
                ctx.hops.push(rpc::NetworkHop {
                    observer: Participant::Worker(manager.get_id().await?).into(),
                    delta_time_ms: (ctx.initiated_at - Utc::now().timestamp_millis() as u64) as u16,
                });

                let leader = manager.get_leader().await?;
                if let LeaderSituation::Connected(leader) = leader {
                    let response = leader
                        .execute(Signal::new(
                            rpc::leader::Request::WorkerProxy(Request::ProcessQuery(query.clone())),
                            Some(ctx.clone()),
                        ))
                        .await?;
                    if let Signal {
                        payload: rpc::leader::Response::WorkerProxy(Response::Query(_product)),
                        ..
                    } = response
                    {
                        product.merge(_product)?;
                    } else if let Signal {
                        payload: rpc::leader::Response::WorkerProxy(Response::Empty),
                        ..
                    } = response
                    {
                        // no other workers connected to leader
                    } else {
                        // unexpected response
                        println!("unexpected response: {:?}", response);
                    }
                } else {
                    // leader not connected
                }
            }

            let local = {
                let query = query.clone();
                tokio::spawn(async move { manager.process_query(query.clone()).await })
            };

            if !workers.is_empty() {
                let mut set = JoinSet::new();
                println!("worker: query: num of remote workers: {}", workers.len());
                for (id, worker) in workers {
                    let query = query.clone();
                    set.spawn(async move {
                        worker
                            .execute(rpc::worker::Request::ProcessQuery(query))
                            .await
                    });
                }
                while let Some(res) = set.join_next().await {
                    let response = res.map_err(|e| Error::NetworkError(format!("{e}")))??;
                    match response {
                        Response::Query(_product) => product.merge(_product)?,
                        _ => return Err(Error::UnexpectedResponse(response.to_string())),
                    }
                }
            }

            let local = local.await.map_err(|e| Error::Other(format!("{e}")))??;
            // println!(
            //     "worker[{my_id}]: global query: product: {product:?}, local: {local:?}"
            // );
            product.merge(local)?;
        }
        query::Scope::LocalWorker => {
            product = manager.process_query(query).await?;
        }
        _ => unimplemented!(),
    }

    Ok(product)
}
