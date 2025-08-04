use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use fnv::FnvHashMap;
use id_pool::IdPool;
use tokio::sync::mpsc::{self};
use tokio::sync::{watch, Mutex};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::executor::{self, Executor, ExecutorMulti, LocalExec, Signal};
use crate::net::ConnectionOrAddress;
use crate::net::{CompositeAddress, Encoding, Transport};
use crate::rpc::msg::{self, DataPullRequest, Message, PullRequestData};
use crate::rpc::server::{RequestLocal, Response};
use crate::rpc::Caller;
use crate::service::Service;
use crate::time::Instant;
use crate::util_net::{decode, encode};
use crate::worker::{WorkerExec, WorkerId};
use crate::{net, rpc, string, worker, Address, Error, EventName, Query, Result};

mod turn;

mod handlers;
mod handlers_compat;

#[cfg(feature = "http_server")]
mod http;

/// Client identification also serving as an access token.
pub type ClientId = Uuid;
/// Server identification.
pub type ServerId = Uuid;

/// Connected client as seen by server.
#[derive(Clone, Debug)]
pub struct Client {
    pub id: ClientId,

    /// IP address of the client.
    pub addr: Option<SocketAddr>,

    /// Currently applied encoding as negotiated with the client.
    pub encoding: Encoding,

    /// Blocking client has to explicitly agree to let server continue stepping
    /// forward, while non-blocking client is more of a passive observer.
    pub is_blocking: bool,
    /// Watch channel for specifying blocking conditions for the client.
    /// Specifically it defines until what clock value the client is allowing
    /// execution. `None` means
    pub unblocked_until: (watch::Sender<Option<usize>>, watch::Receiver<Option<usize>>),

    /// Furthest simulation step client has announced it's ready to proceed to.
    /// If this is bigger than the current step that client counts as
    /// ready for processing to next common furthest step.
    pub furthest_step: usize,

    /// Client-specific keepalive value, if none server config value applies.
    pub keepalive: Option<Duration>,
    pub last_event: Instant,

    /// Authentication pair used by the client.
    pub auth_pair: Option<(String, String)>,
    /// Self-assigned name.
    pub name: String,

    /// List of scheduled data transfers.
    pub scheduled_transfers: FnvHashMap<EventName, Vec<msg::DataTransferRequest>>,
    // /// List of scheduled queries.
    // pub scheduled_queries: FnvHashMap<EventName, Vec<(TaskId, Query)>>,
    /// Clock step on which client needs to be notified of step advance success.
    pub scheduled_advance_response: Option<usize>,

    pub order_store: FnvHashMap<u32, Vec<Address>>,
    pub order_id_pool: IdPool,
}

impl Client {
    // pub fn send_msg() -> Result<()> {
    //
    // }

    pub fn push_event_triggered_query(&mut self, event: EventName, query: Query) -> Result<()> {
        unimplemented!();
        // info!("pushing event triggered query for event: {}", event);
        // if !self.scheduled_queries.contains_key(&event) {
        //     self.scheduled_queries.insert(event.clone(), Vec::new());
        // }
        // self.scheduled_queries.get_mut(&event).unwrap().push(query);
        Ok(())
    }
}

#[derive(Clone)]
pub struct Worker {
    pub exec: WorkerExec,
    /// Unique self-assigned id, used for authenticating with worker.
    pub server_id: ServerId,
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
                    .execute(sig.originating_at(Caller::Server(self.server_id)))
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

#[async_trait::async_trait]
impl ExecutorMulti<Signal<rpc::worker::Request>, Result<Signal<rpc::worker::Response>>> for Worker {
    async fn execute_to_multi(
        &self,
        sig: Signal<rpc::worker::Request>,
    ) -> Result<executor::Receiver<Result<Signal<rpc::worker::Response>>>> {
        match &self.exec {
            WorkerExec::Remote(remote_exec) => {
                remote_exec
                    .execute_to_multi(sig.originating_at(Caller::Server(self.server_id)))
                    .await
            }
            WorkerExec::Local(local_exec) => {
                let sig = Signal::new(sig.payload.into(), sig.ctx);
                local_exec.execute_to_multi(sig).await
            }
        }
    }
}

/// Configuration settings for server.
#[derive(Clone)]
pub struct Config {
    pub listeners: Vec<CompositeAddress>,
    /// Name of the server
    pub name: String,
    /// Description of the server
    pub description: String,

    /// Time since last traffic from any client until server is shutdown,
    /// set to none to keep alive forever
    pub self_keepalive: Option<Duration>,
    /// Time between polls in the main loop
    pub poll_wait: Duration,
    /// Delay between polling for new incoming client connections
    pub accept_delay: Duration,

    /// Time since last traffic from client until connection is terminated
    pub client_keepalive: Option<Duration>,
    /// Compress outgoing messages
    pub use_compression: bool,

    /// Whether to require authorization of incoming clients
    pub use_auth: bool,
    /// User and password pairs for client authorization
    pub auth_pairs: Vec<(String, String)>,

    pub require_client_registration: bool,

    // TODO: additional configuration specific to the http adapter.
    pub http: HttpConfig,

    /// List of transports supported for client connections
    pub transports: Vec<Transport>,
    /// List of encodings supported for client connections
    pub encodings: Vec<Encoding>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            listeners: vec![],

            name: "".to_string(),
            description: "".to_string(),
            self_keepalive: None,
            poll_wait: Duration::from_millis(1),
            accept_delay: Duration::from_millis(200),

            client_keepalive: Some(Duration::from_secs(4)),
            use_compression: false,

            use_auth: false,
            auth_pairs: Vec::new(),

            require_client_registration: false,

            http: HttpConfig::default(),

            transports: vec![
                Transport::FramedTcp,
                #[cfg(feature = "zmq_transport")]
                Transport::ZmqTcp,
            ],
            encodings: vec![
                Encoding::Bincode,
                #[cfg(feature = "msgpack_encoding")]
                Encoding::MsgPack,
            ],
        }
    }
}

#[derive(Clone)]
pub struct HttpConfig {}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {}
    }
}

// TODO add an optional http interface to the server as a crate feature
/// Connection entry point for clients.
///
/// # Network interface overview
///
/// Server's main job is keeping track of the connected `Client`s and handling
/// any requests they may send it's way. It also provides a pipe-like, one-way
/// communication for fast transport of queried data.
///
/// # Listening to client connections
///
/// Server exposes a single stable listener at a known port. Any clients that
/// wish to connect have to send a proper request to that main address. The
/// `accept` function is used to accept new incoming client connections.
/// Here the client is assigned a unique id. Response includes a new address
/// to which client should connect.
///
/// # Initiating client connections
///
/// Server is able not only to receive from, but also to initiate connections
/// to clients. Sent connection request includes the socket address that the
/// client should connect to.
///
/// # Runtime optimizations
///
/// Server capability, in terms of satisfying incoming requests, is determined
/// by it's underlying connection with the simulation.
///
/// Similar to how the inner layer (workers/leader) optimize at runtime,
/// moving entities to achieve ever better system performance, the outer layer
/// (servers/clients) does runtime optimization as well. Clients can be
/// redirected to different servers based on their interest in particular
/// entities and distance/latency incurred when querying those from a
/// particular entry-point (server).
pub struct Server {
    pub id: ServerId,

    /// Server configuration
    pub config: Config,

    /// Connection with an entry-point to the underlying simulation system
    pub worker: Option<Worker>,

    /// Map of all clients by their unique identifier.
    pub clients: FnvHashMap<ClientId, Client>,

    /// Time of creation of this server
    pub started_at: Instant,

    /// Time since last message received
    last_msg_time: Instant,
    /// Time since last new client connection accepted
    last_accept_time: Instant,

    pub services: Vec<Service>,

    pub clock: (watch::Sender<usize>, watch::Receiver<usize>),
    pub blocked: (watch::Sender<u8>, watch::Receiver<u8>),
}

#[derive(Clone)]
pub struct Handle {
    pub ctl: LocalExec<Signal<rpc::server::RequestLocal>, Result<Signal<rpc::server::Response>>>,

    pub client: LocalExec<(Option<ClientId>, rpc::msg::Message), rpc::msg::Message>,
    pub client_id: Option<ClientId>,

    pub worker: LocalExec<Signal<rpc::server::RequestLocal>, Result<Signal<rpc::server::Response>>>,
    pub worker_id: Option<WorkerId>,
    // pub listeners: Vec<CompositeAddress>,
}

#[async_trait::async_trait]
impl Executor<Message, Message> for Handle {
    async fn execute(&self, msg: Message) -> Result<Message> {
        self.client
            .execute((self.client_id, msg))
            .await
            .map_err(|e| e.into())
    }
}

impl Handle {
    /// Connects server to worker.
    pub async fn connect_to_worker(
        &mut self,
        worker_handle: &worker::Handle,
        duplex: bool,
    ) -> Result<()> {
        let mut _server_id = None;

        // connect server to worker
        if let rpc::server::Response::ConnectToWorker { server_id } = self
            .ctl
            .execute(Signal::from(rpc::server::RequestLocal::ConnectToWorker(
                worker_handle.server_exec.clone(),
                self.worker.clone(),
            )))
            .await??
            .into_payload()
        {
            _server_id = Some(server_id);
        }

        if duplex {
            worker_handle.connect_to_local_server(&self).await?;
        }

        Ok(())
    }
}

/// Spawns a new server using provided address and config.
pub fn spawn(
    config: Config,
    mut worker_handle: worker::Handle,
    mut cancel: CancellationToken,
) -> Result<Handle> {
    let cancel = cancel.child_token();

    let (local_ctl_executor, mut local_ctl_stream, _) =
        LocalExec::<Signal<RequestLocal>, Result<Signal<Response>>>::new(20);
    let (local_worker_executor, mut local_worker_stream, _) =
        LocalExec::<Signal<RequestLocal>, Result<Signal<Response>>>::new(20);
    let (local_client_executor, mut local_client_stream, mut local_client_stream_multi) =
        LocalExec::new(20);
    let (net_client_executor, mut net_client_stream, mut net_client_stream_multi) =
        LocalExec::new(20);

    // Web server can be enabled to serve data through an http endpoint.
    // NOTE: can't hide the stream behind a feature because we must expose
    // it later in the tokio::select! macro.
    let (http_client_executor, mut http_client_stream, _) = LocalExec::new(20);
    // TODO: support multiple http listeners
    #[cfg(feature = "http_server")]
    let http_listener = config
        .listeners
        .iter()
        .find(|l| l.transport == Some(Transport::Http));
    // Only spawn the http server if
    #[cfg(feature = "http_server")]
    if let Some(listener) = http_listener {
        let _ = http::spawn(listener.clone(), http_client_executor, cancel.clone());
    }

    // Server operates multiple listeners, each on different transport.
    // Listeners run on separate tasks.
    // Once direct connection is established, encoding is negotiated.
    net::spawn_listeners(&config.listeners, net_client_executor, cancel.clone())?;

    info!("spawning server task, listeners: {:?}", config.listeners);

    // Spawn the server structure
    let mut server = Arc::new(Mutex::new(Server {
        id: Uuid::new_v4(),
        worker: None,
        config,
        clients: Default::default(),
        started_at: Instant::now(),
        last_msg_time: Instant::now(),
        last_accept_time: Instant::now(),
        services: vec![],
        clock: tokio::sync::watch::channel(0 as usize),
        blocked: tokio::sync::watch::channel(0),
    }));

    // Spawn the blocking monitor task.
    let _server = server.clone();
    tokio::spawn(async move {
        // Each iteration watches for changes to the `blocked` watch and
        // notifies the worker accordingly.
        let mut blocked_rcv = _server.lock().await.blocked.1.clone();
        let mut worker = _server.lock().await.worker.clone();
        loop {
            if let Ok(_) = blocked_rcv.changed().await {
                let is_blocked_by = *blocked_rcv.borrow();
                println!("borrowed: is blocked by n clients: {is_blocked_by}");
                if let Some(worker) = worker.as_ref() {
                    trace!(
                        "letting worker know server is not blocking: is_blocked_by: {}",
                        is_blocked_by
                    );
                    if let Err(e) = worker
                        .execute(Signal::from(rpc::worker::Request::SetBlocking(
                            is_blocked_by != 0,
                        )))
                        .await
                    {
                        error!("{}", e);
                    }
                } else {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    worker = if let Ok(s) = _server.try_lock() {
                        s.worker.clone()
                    } else {
                        continue;
                    }
                }
            }
        }
    });

    // Finally spawn the main handler task.
    tokio::spawn(async move {
        loop {
            let mut worker_handle_ = worker_handle.clone();
            tokio::select! {
                Some((sig, s)) = local_ctl_stream.next() => {
                    debug!("handling local ctl request");
                    let server = server.clone();
                    tokio::spawn(async move {
                        let sig = handle_local_ctl_request(sig.payload, sig.ctx, server).await;
                        s.send(sig);
                    });
                },
                Some((sig, s)) = local_worker_stream.next() => {
                    debug!("handling local worker request");
                    let server = server.clone();
                    tokio::spawn(async move {
                        let sig = handle_local_worker_request(sig.payload, sig.ctx, server).await;
                        s.send(sig);
                    });
                },
                Some(((client_id, msg), s)) = local_client_stream.next() => {
                    debug!("handling local client msg: {:?}", msg);
                    let server = server.clone();
                    tokio::spawn(async move {
                        match handle_local_client_request(client_id, msg, server).await {
                            Ok(Some(resp)) => s.send(resp),
                            Ok(None) => Ok(()),
                            Err(e) => s.send(Message::ErrorResponse(e.to_string())),
                        }
                    });
                },
                Some(((client_id, msg), s)) = local_client_stream_multi.next() => {
                    debug!("handling local client msg: {:?}", msg);
                    let server = server.clone();
                    let client_id = client_id.clone();
                    tokio::spawn(async move {
                        let resp = handle_message(client_id, None, msg, server.clone(), Some(s.clone()), None).await;

                        // In case of any errors rewrite the response as
                        // `Message::ErrorResponse`.
                        let resp = match resp {
                            Ok(Some(msg)) => msg,
                            Ok(None) => return,
                            Err(e) => {
                                warn!("{:?}", e);
                                Message::ErrorResponse(format!("{:?}", e))
                            }
                        };
                        s.send(resp);
                    });
                },
                Some(((caller, msg_bytes), s)) = net_client_stream.next() => {
                    debug!("handling net client msg");
                    let server = server.clone();
                    tokio::spawn(async move {
                        // TODO: we're needlessly calling find_client_id twice,
                        // once here and once inside the handler.
                        let encoding = find_client_id(&caller, server.clone()).await.map(|(enc, _)| enc).unwrap_or_default();
                        let resp = handle_net_client_message(caller, msg_bytes, server, None).await;

                        // In case of any errors rewrite the response as
                        // `Message::ErrorResponse`.
                        let resp = match resp {
                            Ok(Some(msg)) => msg,
                            Ok(None) => return,
                            Err(e) => {
                                warn!("{:?}", e);
                                Message::ErrorResponse(format!("{:?}", e))
                            }
                        };
                        let bytes = encode(resp, encoding).unwrap();
                        s.send(bytes);
                    });
                },
                Some(((caller, msg_bytes), s)) = net_client_stream_multi.next() => {
                    trace!("handling net client msg (multi)");
                    let server = server.clone();
                    tokio::spawn(async move {
                        // TODO: we're needlessly calling find_client_id twice,
                        // once here and once inside the handler.
                        let encoding = find_client_id(&caller, server.clone()).await.map(|(enc, _)| enc).unwrap_or_default();
                        let resp = handle_net_client_message(caller, msg_bytes, server, Some(s.clone())).await;

                        // In case of any errors rewrite the response as
                        // `Message::ErrorResponse`.
                        let resp = match resp {
                            Ok(Some(msg)) => msg,
                            Ok(None) => return,
                            Err(e) => {
                                warn!("{:?}", e);
                                Message::ErrorResponse(format!("{:?}", e))
                            }
                        };
                        let bytes = encode(resp, encoding).unwrap();
                        s.send(bytes);
                    });
                },
                Some(((addr, msg), s)) = http_client_stream.next() => {
                    let server = server.clone();
                    tokio::spawn(async move {
                        let resp = handle_http_client_message(addr, msg, server).await;
                        let resp = match resp {
                            Ok(Some(msg)) => Ok(msg),
                            Ok(None) => return,
                            Err(e) => {
                                warn!("{:?}", e);
                                Ok(Message::ErrorResponse(format!("{:?}", e)))
                            }
                        };
                        s.send(resp);
                    });
                },
                _ = cancel.cancelled() => break,
            };
        }
    });

    Ok(Handle {
        ctl: local_ctl_executor,
        client: local_client_executor,
        // listeners,
        client_id: None,
        worker: local_worker_executor,
        worker_id: None,
    })
}

async fn handle_local_ctl_request(
    req: rpc::server::RequestLocal,
    ctx: Option<rpc::Context>,
    mut server: Arc<Mutex<Server>>,
) -> Result<Signal<rpc::server::Response>> {
    match req {
        rpc::server::RequestLocal::ConnectToWorker(server_worker, worker_server) => {
            let server_id = server.lock().await.id;
            let mut resp = server_worker
                .execute(Signal::from(rpc::worker::RequestLocal::IntroduceServer(
                    server_id,
                    worker_server,
                )))
                .await?
                .map_err(|e| Error::FailedConnectingServerToWorker(e.to_string()))?
                .into_payload();

            if let rpc::worker::Response::Register { server_id } = resp {
                server.lock().await.worker = Some(Worker {
                    exec: WorkerExec::Local(server_worker),
                    server_id,
                });
                trace!("set the worker with server id: {}", server_id);
                Ok(Signal::new(
                    rpc::server::Response::ConnectToWorker { server_id },
                    ctx,
                ))
            } else {
                Err(Error::UnexpectedResponse("".to_string()))
            }
        }
        rpc::server::RequestLocal::Request(req) => handle_ctl_request(req, ctx, server).await,
        _ => todo!(),
    }
}

async fn handle_ctl_request(
    req: rpc::server::Request,
    ctx: Option<rpc::Context>,
    mut server: Arc<Mutex<Server>>,
) -> Result<Signal<rpc::server::Response>> {
    use rpc::server::{Request as ServerRequest, Response as ServerResponse};
    match req {
        ServerRequest::Status => {
            // let scenario = server
            //     .lock()
            //     .await
            //     .worker
            //     .as_ref()
            //     .expect("server not connected to worker")
            //     .execute(Request::Scenario)
            //     .await?;
            // println!(">> scenario: {:?}", scenario);
            Ok(Signal::new(ServerResponse::Status { uptime: 66 }, ctx))
        }
        // ServerRequest::UploadProject(project) => {
        //     // propagate request to the connected worker
        //     let resp = server
        //         .lock()
        //         .await
        //         .worker
        //         .as_ref()
        //         .ok_or(Error::WorkerNotConnected("".to_string()))?
        //         .execute(Request::PullModel(project))
        //         .await?;
        //     println!("upload project worker resp: {:?}", resp);
        //     Ok(ServerResponse::UploadProject { success: true })
        // }
        // ServerRequest::Message(msg) => {
        //     let resp = handle_message(client_id, None, msg, server, runtime).await;
        //     s.send(resp.map(|msg| ServerResponse::Message(msg)));
        // }
        _ => unimplemented!("request: {req:?}"),
    }
}

async fn handle_local_worker_request(
    req: rpc::server::RequestLocal,
    ctx: Option<rpc::Context>,
    server: Arc<Mutex<Server>>,
) -> Result<Signal<rpc::server::Response>> {
    match req {
        rpc::server::RequestLocal::IntroduceWorker(server_worker) => {
            let server_id = server.lock().await.id;
            let worker = Worker {
                exec: WorkerExec::Local(server_worker),
                server_id,
            };
            server.lock().await.worker = Some(worker);
            Ok(Signal::new(
                rpc::server::Response::IntroduceWorker(server_id),
                ctx,
            ))
        }
        rpc::server::RequestLocal::Request(req) => {
            let worker_id = ctx.as_ref().map(|c| c.origin.id());
            handle_worker_request(req, ctx, worker_id, server).await
        }
        _ => todo!(),
    }
}

async fn handle_worker_request(
    req: rpc::server::Request,
    ctx: Option<rpc::Context>,
    worker_id: Option<WorkerId>,
    server: Arc<Mutex<Server>>,
) -> Result<Signal<rpc::server::Response>> {
    debug!("server: handling worker request: {req}");
    match req {
        rpc::server::Request::Redirect => {
            unimplemented!();
        }
        rpc::server::Request::ClockChangedTo(clock) => {
            server.lock().await.clock.0.send(clock);
            Ok(Signal::new(rpc::server::Response::Empty, ctx))
        }
        _ => unimplemented!(),
    }
}

async fn handle_local_client_request(
    client_id: Option<ClientId>,
    msg: Message,
    server: Arc<Mutex<Server>>,
) -> Result<Option<Message>> {
    handle_message(client_id, None, msg, server.clone(), None, None).await
}

async fn handle_http_client_message(
    caller: ConnectionOrAddress,
    msg: Message,
    server: Arc<Mutex<Server>>,
) -> Result<Option<Message>> {
    let (encoding, client) = find_client_id(&caller, server.clone()).await?;

    let peer_addr = match caller {
        ConnectionOrAddress::Connection(connection) => None,
        ConnectionOrAddress::Address(socket_addr) => Some(socket_addr),
    };

    debug!("handling web client msg: {:?}", msg);
    let resp = handle_message(client.map(|c| c.id), peer_addr, msg, server, None, None).await;
    let resp = match resp {
        Ok(Some(msg)) => msg,
        Ok(None) => return Ok(None),
        Err(e) => {
            warn!("{:?}", e);
            Message::ErrorResponse(format!("{:?}", e))
        }
    };

    Ok(Some(resp))
}

async fn handle_net_client_message(
    caller: ConnectionOrAddress,
    bytes: Vec<u8>,
    server: Arc<Mutex<Server>>,
    resp_stream_bytes: Option<mpsc::Sender<Vec<u8>>>,
) -> Result<Option<Message>> {
    let (encoding, client) = find_client_id(&caller, server.clone()).await?;

    let _server = server.lock().await;
    let msg: Message = decode(bytes.as_slice(), encoding)?;
    trace!("handling network msg: {:?}", msg);

    let client_id = client.map(|c| c.id);

    drop(_server);

    let caller_addr = match caller {
        ConnectionOrAddress::Address(addr) => Some(addr),
        ConnectionOrAddress::Connection(conn) => Some(conn.remote_address()),
    };

    handle_message(
        client_id,
        caller_addr,
        msg,
        server.clone(),
        None,
        resp_stream_bytes,
    )
    .await
}

/// Handles an incoming `Message`.
async fn handle_message(
    client_id: Option<ClientId>,
    peer_addr: Option<SocketAddr>,
    msg: Message,
    server: Arc<Mutex<Server>>,
    resp_stream: Option<mpsc::Sender<Message>>,
    resp_stream_bytes: Option<mpsc::Sender<Vec<u8>>>,
) -> Result<Option<Message>> {
    let client_id = match msg {
        Message::RegisterClientRequest(req) => {
            // TODO auth incoming clients

            let id = Uuid::new_v4();

            // TODO support transport and encoding negotiation
            let client = Client {
                id,
                addr: peer_addr,
                encoding: Encoding::Bincode,
                is_blocking: req.is_blocking,
                // Set to `None` means client is blocking.
                unblocked_until: watch::channel((!req.is_blocking).then_some(0)),
                furthest_step: 0,
                keepalive: None,
                last_event: Instant::now(),
                auth_pair: None,
                name: req.name,
                scheduled_transfers: Default::default(),
                scheduled_advance_response: None,
                order_store: Default::default(),
                order_id_pool: IdPool::new(),
            };

            if client.is_blocking {
                server.lock().await.blocked.0.send_modify(|c| *c += 1);
            }

            server.lock().await.clients.insert(id, client);

            if let Some(peer_addr) = peer_addr {
                // HACK
                // server.lock().await.clients_by_addr.insert(peer_addr, id);
            }

            return Ok(Some(Message::RegisterClientResponse(
                msg::RegisterClientResponse {
                    client_id: id.to_string(),
                    encoding: Encoding::Bincode,
                    transport: Transport::FramedTcp,
                    redirect_to: None,
                },
            )));
        }
        _ => {
            if client_id.is_none() && server.lock().await.config.require_client_registration {
                // Client id is required but cannot be determined. Return a hard error.
                return Err(Error::Forbidden("client not recognized".to_string()));
            } else if client_id.is_none() {
                // If client id is not known and at the same time not required,
                // then we provide the caller with Uuid::nil(). This allows for
                // quickly setting up unidentified access for callers.
                Uuid::nil()
            } else {
                // Otherwise client id must be known, just unwrap it.
                client_id.unwrap()
            }
        }
    };

    match msg {
        Message::OK => Ok(Some(Message::OK)),
        Message::Disconnect => {
            let mut server = server.lock().await;
            if let Some(client) = server.clients.get(&client_id) {
                if client.is_blocking {
                    if *server.blocked.0.borrow() != 0 {
                        server.blocked.0.send_modify(|c| *c -= 1);
                    }
                }
            }

            server.clients.remove(&client_id);
            println!("disconnected client");

            Ok(Some(Message::Disconnect))
        }
        Message::StatusRequest(req) => {
            // use server_worker::{Request, Response, RequestLocal};
            // server.worker.server_exec.execute(RequestLocal::Request(Request::))

            let mut _server = server.lock().await;
            // let clock = *_server.clock.1.borrow();

            _server
                .handle_status_request(req, &client_id)
                .await
                .map(|msg| Some(msg))
        }
        Message::AdvanceRequest(req) => {
            debug!("server got step request: step_count {}", req.step_count);

            let resp = turn::handle_advance_request(server, req, client_id).await;
            debug!("server handled step request: {:?}", resp);
            resp.map(|msg| Some(msg))
        }
        Message::QueryRequest(q) => {
            let server_id = server.lock().await.id;
            if let Some(worker) = server.lock().await.worker.as_ref() {
                let resp = worker
                    .execute(
                        Signal::from(rpc::worker::Request::ProcessQuery(q))
                            .originating_at(Caller::Server(server_id)),
                    )
                    .await?
                    .into_payload();
                if let rpc::worker::Response::Query(qp) = resp {
                    return Ok(Some(Message::QueryResponse(qp)));
                }
            }
            return Err(Error::Unknown);
        }
        Message::EntityListRequest => {
            if let Some(worker) = server.lock().await.worker.as_ref() {
                if let rpc::worker::Response::EntityList(entities) = worker
                    .execute(Signal::from(rpc::worker::Request::EntityList))
                    .await?
                    .into_payload()
                {
                    return Ok(Some(Message::EntityListResponse(entities)));
                }
            }
            return Err(Error::Unknown);
        }
        Message::PingRequest(vec) => Ok(Some(Message::PingResponse(vec))),
        Message::ErrorResponse(_) => todo!(),
        Message::PingResponse(vec) => todo!(),
        Message::EntityListResponse(vec) => todo!(),
        Message::RegisterClientRequest(register_client_request) => todo!(),
        Message::RegisterClientResponse(register_client_response) => todo!(),
        Message::StatusResponse(status_response) => todo!(),
        Message::AdvanceResponse(advance_response) => todo!(),
        Message::QueryResponse(query_product) => todo!(),
        Message::SpawnEntitiesRequest(spawn_entities_request) => todo!(),
        Message::SpawnEntitiesResponse(spawn_entities_response) => todo!(),
        Message::DataPullRequest(data_pull_request) => {
            let mut server = server.lock().await;
            if let Some(worker) = &server.worker {
                if let DataPullRequest {
                    data: PullRequestData::AddressedVars(pairs),
                } = data_pull_request
                {
                    worker
                        .execute(Signal::from(rpc::worker::Request::SetVars(pairs)))
                        .await?;
                }

                Ok(Some(Message::DataPullResponse(msg::DataPullResponse {
                    error: "".to_owned(),
                })))
            } else {
                Err(Error::Unknown)
            }
        }
        Message::DataPullResponse(data_pull_response) => todo!(),
        Message::TypedDataPullRequest(typed_data_pull_request) => todo!(),
        Message::TypedDataPullResponse(typed_data_pull_response) => todo!(),
        Message::ExportSnapshotRequest(export_snapshot_request) => todo!(),
        Message::ExportSnapshotResponse(export_snapshot_response) => todo!(),
        Message::UploadProjectArchiveRequest(upload_project_request) => todo!(),
        Message::UploadProjectArchiveResponse(upload_project_response) => todo!(),
        Message::ListScenariosRequest(list_scenarios_request) => todo!(),
        Message::ListScenariosResponse(list_scenarios_response) => todo!(),
        Message::LoadScenarioRequest(load_scenario_request) => todo!(),
        Message::LoadScenarioResponse(load_scenario_response) => todo!(),
        Message::InitializeRequest => todo!(),
        Message::InitializeResponse => todo!(),
        Message::SubscribeResponse(id) => todo!(),
        Message::SubscribeRequest(triggers, query) => {
            if resp_stream_bytes.is_none() && resp_stream.is_none() {
                return Err(Error::Other("streaming context unavailable".to_owned()));
            }

            let mut worker_recv = server
                .lock()
                .await
                .worker
                .as_ref()
                .unwrap()
                .execute_to_multi(Signal::from(rpc::worker::Request::Subscribe(
                    triggers, query,
                )))
                .await
                .unwrap();

            while let Ok(Some(sig)) = worker_recv.recv().await {
                if let Ok(Signal { payload, .. }) = sig {
                    match payload {
                        rpc::worker::Response::Query(product) => {
                            let msg = Message::QueryResponse(product);
                            if let Some(sender) = &resp_stream_bytes {
                                sender
                                    .send(encode(
                                        msg,
                                        // TODO: use client-defined encoding.
                                        Encoding::Bincode,
                                    )?)
                                    .await;
                            } else if let Some(sender) = &resp_stream {
                                sender.send(msg).await;
                            }
                        }
                        rpc::worker::Response::Subscribe(id) => {
                            let msg = Message::SubscribeResponse(id);
                            if let Some(sender) = &resp_stream_bytes {
                                sender
                                    // TODO: use client-defined encoding.
                                    .send(encode(msg, Encoding::Bincode)?)
                                    .await
                                    .unwrap();
                            } else if let Some(sender) = &resp_stream {
                                sender.send(msg).await.unwrap();
                            }
                        }
                        _ => unimplemented!(),
                    }
                }
            }

            // Don't send any additional message at the end.
            Ok(None)
        }
        Message::UnsubscribeRequest(id) => {
            if let Some(worker) = &server.lock().await.worker {
                let sig = worker
                    .execute(Signal::from(rpc::worker::Request::Unsubscribe(id)))
                    .await?;
                sig.payload.ok()?;
                Ok(Some(Message::OK))
            } else {
                Err(Error::WorkerNotConnected("".to_owned()))
            }
        }
    }
}

// TODO: consider returning just the client id instead of the whole thing.
async fn find_client_id(
    caller: &ConnectionOrAddress,
    server: Arc<Mutex<Server>>,
) -> Result<(Encoding, Option<Client>)> {
    match &caller {
        ConnectionOrAddress::Address(addr) => {
            trace!("looking for client with addr: {addr}");
            // trace!(
            //     "all clients: {:?}",
            //     server
            //         .lock()
            //         .await
            //         .clients
            //         .iter()
            //         .map(|(_, c)| c.addr)
            //         .collect::<Vec<_>>()
            // );
            match server
                .lock()
                .await
                .clients
                .iter()
                .find(|(_, c)| c.addr.as_ref() == Some(addr))
            {
                Some((_, client)) => {
                    trace!(
                        "found client by addr: {addr}, encoding: {}",
                        client.encoding
                    );
                    Ok((client.encoding, Some(client.clone())))
                }
                None => {
                    trace!("client not found");
                    Ok((Encoding::Bincode, None))
                }
            }
        }
        ConnectionOrAddress::Connection(connection) => {
            let addr = connection.remote_address();
            // println!("server clients: {:?}", _server.clients);
            // println!("current connection addr: {addr}");
            match server
                .lock()
                .await
                .clients
                .iter()
                .find(|(_, c)| c.addr == Some(addr))
            {
                Some((_, client)) => {
                    trace!(
                        "found client by addr: {addr}, encoding: {}",
                        client.encoding
                    );
                    Ok((client.encoding, Some(client.clone())))
                }
                None => {
                    trace!("client not found");
                    Ok((Encoding::Bincode, None))
                }
            }
        }
    }
}

impl Server {
    /// Initializes services based on the available model.
    ///
    /// # New services with model changes
    ///
    /// Can be called repeatedly to initialize services following model
    /// changes.
    pub fn initialize_services(&mut self) -> Result<()> {
        // match &mut self.sim {
        // SimCon::Local(sim) => {
        //     // start the service processes
        //     for service_model in &sim.model.services {
        //         if self
        //             .services
        //             .iter()
        //             .find(|s| s.name == service_model.name)
        //             .is_none()
        //         {
        //             info!("starting service: {}", service_model.name);
        //             let service = Service::start_from_model(
        //                 service_model.clone(),
        //                 // TODO hack
        //                 "".to_string(),
        //                 // self.greeters
        //                 //     .first()
        //                 //     .unwrap()
        //                 //     .listener_addr(None)?
        //                 //     .to_string(),
        //             )?;
        //             self.services.push(service);
        //         }
        //     }
        // }
        // SimCon::Worker(worker) => {
        //     if let Some(node) = &worker.sim_node {
        //         for service_model in &node.model.services {
        //             if self
        //                 .services
        //                 .iter()
        //                 .find(|s| s.name == service_model.name)
        //                 .is_none()
        //             {
        //                 info!("starting service: {}", service_model.name);
        //                 let service = Service::start_from_model(
        //                     service_model.clone(),
        //                     // TODO hack
        //                     "".to_string(),
        //                     // self.greeters
        //                     //     .first()
        //                     //     .unwrap()
        //                     //     .listener_addr(None)?
        //                     //     .to_string(),
        //                 )?;
        //                 self.services.push(service);
        //             }
        //         }
        //     }
        // }
        // SimCon::Leader(org) => {
        //     // warn!("not starting any services since it's a
        //     // leader-backed server");
        // }
        // }

        Ok(())
    }

    /// This function handles shutdown cleanup, like stopping spawned services.
    pub fn cleanup(&mut self) -> Result<()> {
        for service in &mut self.services {
            // service.stop();
        }
        Ok(())
    }

    // async fn handle_compat_message(
    //     &mut self,
    //     msg: compat::Message,
    //     client_id: &ClientId,
    // ) -> Result<()> {
    //     println!("handling compat message: {:?}", msg);
    //     match msg.type_ {
    //         // MessageType::QueryRequest => self.handle_query_request_compat(msg, client_id),
    //         _ => self.handle_message(msg.into(), client_id).await,
    //     }
    // }
    //
    // async fn handle_message(&mut self, msg: Message, client_id: &ClientId) -> Result<()> {
    //     let response = match msg {
    //         // Message::Heartbeat => (),
    //         Message::RegisterClientRequest(req) => {
    //             tokio::time::sleep(Duration::from_secs(3)).await;
    //             println!(">>>>>>>>> done processing");
    //         }
    //         Message::PingRequest(bytes) => self.handle_ping_request(bytes, client_id)?,
    //         Message::StatusRequest(sr) => self.handle_status_request(sr, client_id).await?,
    //         Message::TurnAdvanceRequest(tar) => {
    //             self.handle_turn_advance_request(tar, client_id).await?
    //         }
    //         // Message::QueryRequestCompat(qr) => self.handle_query_request_compat(msg, client_id)?,
    //         // MessageCompatType::NativeQueryRequest => {
    //         //     self.handle_native_query_request(msg, client_id)?
    //         // }
    //         // MessageCompatType::JsonPullRequest => self.handle_json_pull_request(msg, client_id)?,
    //         Message::DataTransferRequest(dtr) => {
    //             // self.handle_data_transfer_request(dtr, client_id).await?
    //         }
    //         Message::TypedDataTransferRequest(tdtr) => {
    //             // self.handle_typed_data_transfer_request(tdtr, client_id)?
    //         }
    //         Message::DataPullRequest(dpr) => {
    //             // self.handle_data_pull_request(dpr, client_id)?
    //         }
    //         Message::TypedDataPullRequest(tdpr) => {
    //             // self.handle_typed_data_pull_request(tdpr, client_id)?
    //         }
    //         Message::ScheduledDataTransferRequest(sdtr) => {
    //             // self.handle_scheduled_data_transfer_request(sdtr, client_id)?
    //         }
    //         Message::SpawnEntitiesRequest(ser) => {
    //             self.handle_spawn_entities_request(ser, client_id)?
    //         }
    //         Message::ExportSnapshotRequest(esr) => {
    //             self.handle_export_snapshot_request(esr, client_id)?
    //         }
    //         _ => println!("unknown message: {:?}", msg),
    //     };
    //     // self.clients
    //     //     .get_mut(client_id)
    //     //     .unwrap()
    //     //     .connection
    //     //     .send_obj(response, None);
    //     Ok(())
    // }

    pub fn handle_export_snapshot_request(
        &mut self,
        esr: msg::ExportSnapshotRequest,
        client_id: &ClientId,
    ) -> Result<()> {
        let client = self
            .clients
            .get_mut(client_id)
            .ok_or(Error::FailedGettingClientById(client_id.clone()))?;
        // let esr: ExportSnapshotRequest = msg.unpack_payload(client.connection.encoding())?;
        // let snap = match &mut self.sim {
        //     SimCon::Local(sim) => {
        //         if esr.save_to_disk {
        //             sim.save_snapshot(&esr.name, false)?;
        //         }
        //         if esr.send_back {
        //             let resp = Message::ExportSnapshotResponse(ExportSnapshotResponse {
        //                 error: "".to_string(),
        //                 snapshot: vec![],
        //             });
        //             // client.connection.send_obj(resp, None);
        //         }
        //         return Ok(());
        //     }
        //     SimCon::Leader(org) => {
        //         // let task_id = org.init_download_snapshots()?;
        //         // TODO perhaps request separate id for leader and server levels
        //         // self.tasks.insert(
        //         //     task_id,
        //         //     ServerTask::WaitForLeaderSnapshotResponses {
        //         //         client_id: *client_id,
        //         //         compressed: true,
        //         //     },
        //         // );
        //         return Err(Error::WouldBlock);
        //     }
        //     _ => unimplemented!(),
        // };

        // client.connection.send_payload(resp, None)
        Ok(())
    }

    pub fn handle_spawn_entities_request(
        &mut self,
        ser: msg::SpawnEntitiesRequest,
        client_id: &ClientId,
    ) -> Result<()> {
        let client = self.clients.get_mut(client_id).unwrap();
        let mut out_names = Vec::new();
        let mut error = String::new();
        // let ser: SpawnEntitiesRequest = msg.unpack_payload(client.connection.encoding())?;

        for (i, prefab) in ser.entity_prefabs.iter().enumerate() {
            trace!("handling prefab: {}", prefab);
            let entity_name = match ser.entity_names[i].as_str() {
                "" => None,
                _ => Some(string::new_truncate(&ser.entity_names[i])),
            };
            // match &mut self.sim {
            //     SimCon::Local(sim) => {
            //         match sim.spawn_entity_by_prefab_name(
            //             Some(&string::new_truncate(&prefab)),
            //             entity_name,
            //         ) {
            //             Ok(entity_id) => out_names.push(entity_id.to_string()),
            //             Err(e) => error = e.to_string(),
            //         }
            //     }
            //     SimCon::Leader(org) => org.central.spawn_entity(
            //         Some(prefab.into()),
            //         entity_name,
            //         Some(DistributionPolicy::Random),
            //     )?,
            //     _ => unimplemented!(),
            // }
        }
        let resp = Message::SpawnEntitiesResponse(msg::SpawnEntitiesResponse {
            entity_names: out_names,
            error,
        });

        // client.connection.send_obj(resp, None)?;
        Ok(())
    }

    pub fn handle_ping_request(&mut self, bytes: Vec<u8>, client_id: &ClientId) -> Result<()> {
        let client = self.clients.get_mut(client_id).unwrap();
        let resp = Message::PingResponse(bytes);
        // client.connection.send_obj(resp, None);
        Ok(())
    }

    pub async fn handle_status_request(
        &mut self,
        sr: msg::StatusRequest,
        client_id: &ClientId,
    ) -> Result<Message> {
        use rpc::msg::client_server::StatusResponse;
        use rpc::worker::{Request, Response};

        let connected_clients = self.clients.iter().map(|(id, c)| c.name.clone()).collect();
        // let mut client = self
        //     .clients
        //     .get_mut(client_id)
        //     .ok_or(Error::Other("client not available".to_string()))?;

        if let Response::Status {
            id,
            uptime,
            worker_count,
        } = self
            .worker
            .as_ref()
            .ok_or(Error::WorkerNotConnected("".to_string()))?
            .execute(Signal::from(Request::Status))
            .await?
            .into_payload()
        {
            let resp = Message::StatusResponse(StatusResponse {
                name: self.config.name.clone(),
                description: self.config.description.clone(),
                // address: self.greeters.first().unwrap().local_addr()?.to_string(),
                connected_clients,
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                // TODO: explicitly say this is *server* uptime
                uptime: self.started_at.elapsed().as_secs(),
                current_tick: match self
                    .worker
                    .as_ref()
                    .ok_or(Error::WorkerNotConnected("".to_string()))?
                    .execute(Signal::from(Request::Clock))
                    .await
                {
                    Ok(Signal {
                        payload: Response::Clock(clock),
                        ..
                    }) => clock,
                    Err(e) => return Err(e.into()),
                    _ => return Err(Error::Other("wrong response type".to_string())),
                },
                worker: id.simple().to_string(),
            });
            Ok(resp)
        } else {
            unimplemented!()
        }
    }

    // pub async fn handle_data_transfer_request(
    //     &mut self,
    //     dtr: DataTransferRequest,
    //     client_id: &ClientId,
    // ) -> Result<()> {
    //     let mut client = self.clients.get_mut(client_id).unwrap();
    //     let mut data_pack = TypedSimDataPack::empty();
    //     match &mut self.sim {
    //         SimCon::Local(sim_instance) => {
    //             handle_data_transfer_request_local(&dtr, sim_instance, client)?
    //         }
    //         SimCon::Leader(org) => {
    //             let mut vars = FnvHashMap::default();
    //             match dtr.transfer_type.as_str() {
    //                 "Full" => {
    //                     for (worker_id, worker) in &mut org.net.workers {
    //                         worker.connection.send_obj(
    //                             crate::sig::Signal::new(
    //                                 0,
    //                                 *worker_id,
    //                                 TaskId::new_v4(),
    //                                 engine_core::distr::Signal::DataRequestAll,
    //                             ),
    //                             None,
    //                         )?
    //                     }
    //                     for (worker_id, worker) in &mut org.net.workers {
    //                         let (_, sig) = worker.connection.recv_obj::<NetSignal>().await?;
    //                         match sig.into_inner().1 {
    //                             engine_core::distr::Signal::DataResponse(data) => vars.extend(data),
    //                             s => warn!("unhandled signal: {:?}", s),
    //                         }
    //                     }
    //
    //                     let response = Message::DataTransferResponse(DataTransferResponse {
    //                         data: TransferResponseData::Var(VarSimDataPack { vars }),
    //                     });
    //                     // client.connection.send_obj(response, None)?;
    //                 }
    //                 _ => unimplemented!(),
    //             }
    //         }
    //         SimCon::Worker(worker) => {
    //             //TODO
    //             // categorize worker connection to the cluster, whether it's only connected
    //             // to the leader, to leader and to all workers, or some other way
    //             worker
    //                 .net
    //                 .sig_send_central(TaskId::new_v4(), Signal::DataRequestAll)?;
    //
    //             // for (worker_id, worker) in &mut worker.network.comrades {
    //             //     let (_, sig) = worker.connection.recv_sig()?;
    //             //     match sig.into_inner() {
    //             //         bigworlds::distr::Signal::DataResponse(data) => {
    //             //             collection.extend(data)
    //             //         }
    //             //         _ => unimplemented!(),
    //             //     }
    //             // }
    //
    //             let (task_id, resp) = worker.net.sig_read_central().await?;
    //             if let Signal::DataResponse(data_vec) = resp {
    //                 let mut data_pack = VarSimDataPack::default();
    //                 for (addr, var) in data_vec {
    //                     data_pack.vars.insert((addr.0, addr.1, addr.2), var);
    //                 }
    //                 for (entity_id, entity) in &worker.sim_node.as_ref().unwrap().entities {
    //                     for ((comp_name, var_name), var) in &entity.storage.map {
    //                         data_pack.vars.insert(
    //                             (
    //                                 string::new_truncate(&entity_id.to_string()),
    //                                 comp_name.clone(),
    //                                 var_name.clone(),
    //                             ),
    //                             var.clone(),
    //                         );
    //                     }
    //                 }
    //
    //                 let response = Message::DataTransferResponse(DataTransferResponse {
    //                     data: TransferResponseData::Var(data_pack),
    //                 });
    //                 // client.connection.send_obj(response, None)?;
    //             }
    //         }
    //     };
    //
    //     Ok(())
    // }

    // pub fn handle_typed_data_transfer_request(
    //     &mut self,
    //     tdtr: TypedDataTransferRequest,
    //     client_id: &ClientId,
    // ) -> Result<()> {
    //     let mut client = self.clients.get_mut(client_id).unwrap();
    //     let mut data_pack = TypedSimDataPack::empty();
    //     match &mut self.sim {
    //         SimCon::Local(sim_instance) => {
    //             let model = &sim_instance.model;
    //             match tdtr.transfer_type.as_str() {
    //                 "Full" => {
    //                     // let mut data_pack = bigworlds::query::AddressedTypedMap::default();
    //                     let mut data_pack = TypedSimDataPack::empty();
    //                     for (entity_uid, entity) in &sim_instance.entities {
    //                         for ((comp_name, var_id), v) in entity.storage.map.iter() {
    //                             if v.is_float() {
    //                                 data_pack.floats.insert(
    //                                     // format!(
    //                                     //     ":{}:{}:{}:{}",
    //                                     //     // get entity string id if available
    //                                     //     sim_instance
    //                                     //         .entities_idx
    //                                     //         .iter()
    //                                     //         .find(|(e_id, e_idx)| e_idx == &entity_uid)
    //                                     //         .map(|(e_id, _)| e_id.as_str())
    //                                     //         .unwrap_or(entity_uid.to_string().as_str()),
    //                                     //     comp_name,
    //                                     //     VarType::Float.to_str(),
    //                                     //     var_id
    //                                     // ),
    //                                     Address {
    //                                         // get entity string id if available
    //                                         entity: sim_instance
    //                                             .entity_idx
    //                                             .iter()
    //                                             .find(|(e_id, e_idx)| e_idx == &entity_uid)
    //                                             .map(|(e_id, _)| e_id.clone())
    //                                             .unwrap_or(string::new_truncate(
    //                                                 &entity_uid.to_string(),
    //                                             )),
    //                                         // entity: entity_uid.parse().unwrap(),
    //                                         component: comp_name.clone(),
    //                                         var_type: VarType::Float,
    //                                         var_name: var_id.clone(),
    //                                     }
    //                                     .into(),
    //                                     // comp_name.to_string(),
    //                                     *v.as_float().unwrap(),
    //                                 );
    //                             }
    //                         }
    //                     }
    //
    //                     let response =
    //                         Message::TypedDataTransferResponse(TypedDataTransferResponse {
    //                             data: data_pack,
    //                             error: String::new(),
    //                         });
    //                     // client.connection.send_obj(response, None);
    //                 }
    //                 _ => unimplemented!(),
    //             }
    //         }
    //         _ => unimplemented!(),
    //     }
    //     Ok(())
    // }

    // pub fn handle_scheduled_data_transfer_request(
    //     &mut self,
    //     sdtr: ScheduledDataTransferRequest,
    //     client_id: &ClientId,
    // ) -> Result<()> {
    //     let mut client = self
    //         .clients
    //         .get_mut(client_id)
    //         .ok_or(Error::Other("failed getting client".to_string()))?;
    //     for event_trigger in sdtr.event_triggers {
    //         let event_id = string::new(&event_trigger)?;
    //         if !client.scheduled_transfers.contains_key(&event_id) {
    //             client
    //                 .scheduled_transfers
    //                 .insert(event_id.clone(), Vec::new());
    //         }
    //         let dtr = DataTransferRequest {
    //             transfer_type: sdtr.transfer_type.clone(),
    //             selection: sdtr.selection.clone(),
    //         };
    //         client
    //             .scheduled_transfers
    //             .get_mut(&event_id)
    //             .unwrap()
    //             .push(dtr);
    //     }
    //
    //     Ok(())
    // }

    fn handle_single_address(server: &Server) {}

    // pub fn handle_list_local_scenarios_request(
    //     &mut self,
    //     payload: Vec<u8>,
    //     client: &mut Client,
    // ) -> Result<()> {
    //     let req: ListLocalScenariosRequest = decode(&payload, client.connection.encoding())?;
    //     //TODO check `$working_dir/scenarios` for scenarios
    //     //
    //     //
    //
    //     let resp = Message::ListLocalScenariosResponse(ListLocalScenariosResponse {
    //         scenarios: Vec::new(),
    //         error: String::new(),
    //     });
    //     client.connection.send_obj(resp, None)?;
    //     Ok(())
    // }

    // pub fn handle_load_local_scenario_request(
    //     payload: Vec<u8>,
    //     server_arc: Arc<Mutex<Server>>,
    //     client: &mut Client,
    // ) -> Result<()> {
    //     let req: LoadLocalScenarioRequest = decode(&payload, client.connection.encoding())?;
    //
    //     //TODO
    //     //
    //
    //     let resp = Message::LoadLocalScenarioResponse(LoadLocalScenarioResponse {
    //         error: String::new(),
    //     });
    //     client.connection.send_obj(resp, None)?;
    //     Ok(())
    // }

    // pub fn handle_load_remote_scenario_request(
    //     payload: Vec<u8>,
    //     server_arc: Arc<Mutex<Server>>,
    //     client: &mut Client,
    // ) -> Result<()> {
    //     let req: LoadRemoteScenarioRequest = decode(&payload, client.connection.encoding())?;
    //
    //     //TODO
    //     //
    //
    //     let resp = Message::LoadRemoteScenarioResponse(LoadRemoteScenarioResponse {
    //         error: String::new(),
    //     });
    //     client.connection.send_obj(resp, None)?;
    //     Ok(())
    // }
}

// fn handle_data_transfer_request_local(
//     request: &DataTransferRequest,
//     sim: &SimExec,
//     client: &mut Client,
// ) -> Result<()> {
//     let model = &sim.model;
//     match request.transfer_type.as_str() {
//         "Full" => {
//             let mut data_pack = VarSimDataPack::default();
//             for (entity_id, entity) in &sim.entities {
//                 for ((comp_name, var_id), v) in entity.storage.map.iter() {
//                     let mut ent_name = EntityName::from(entity_id.to_string());
//                     if let Some((_ent_name, _)) =
//                         sim.entity_idx.iter().find(|(_, id)| id == &entity_id)
//                     {
//                         ent_name = _ent_name.clone();
//                     }
//                     data_pack.vars.insert(
//                         // format!(
//                         //     "{}:{}:{}:{}",
//                         //     entity_uid,
//                         //     comp_name,
//                         //     v.get_type().to_str(),
//                         //     var_id
//                         // ),
//                         (ent_name, comp_name.clone(), var_id.clone()),
//                         v.clone(),
//                     );
//                 }
//             }
//
//             let response = Message::DataTransferResponse(DataTransferResponse {
//                 data: TransferResponseData::Var(data_pack),
//             });
//             // client.connection.send_obj(response, None)?;
//             println!("sent data transfer response");
//             Ok(())
//         }
//         "Select" => {
//             let mut data_pack = TypedSimDataPack::empty();
//             let mut selected = Vec::new();
//             selected.extend_from_slice(&request.selection);
//
//             // todo handle asterrisk addresses
//             // for address in &dtr.selection {
//             //     if address.contains("*") {
//             //         let addr = Address::from_str(address).unwrap();
//             //         selected.extend(
//             //             addr.expand(sim_instance)
//             //                 .iter()
//             //                 .map(|addr| addr.to_string()),
//             //         );
//             //     }
//             // }
//             for address in &selected {
//                 let address = match Address::from_str(&address) {
//                     Ok(a) => a,
//                     Err(_) => continue,
//                 };
//                 if let Ok(var) = sim.get_var(&address) {
//                     if var.is_float() {
//                         data_pack
//                             .floats
//                             .insert(address.into(), *var.as_float().unwrap());
//                     }
//                 }
//             }
//
//             let response = Message::DataTransferResponse(DataTransferResponse {
//                 data: TransferResponseData::Typed(data_pack),
//             });
//             // client.connection.send_obj(response, None)?;
//             Ok(())
//         }
//         // select using addresses but return data as ordered set without
//         // address keys, order is stored on server under it's own unique id
//         "SelectVarOrdered" => {
//             let mut data = VarSimDataPackOrdered::default();
//             let selection = &request.selection;
//
//             // empty selection means reuse last ordering
//             if selection.is_empty() {
//                 let order_id = 1;
//                 let order = client.order_store.get(&order_id).unwrap();
//                 for addr in order {
//                     if let Ok(var) = sim.get_var(&addr) {
//                         data.vars.push(var.clone());
//                     }
//                 }
//                 let response = Message::DataTransferResponse(DataTransferResponse {
//                     data: TransferResponseData::VarOrdered(order_id, data),
//                 });
//                 // client.connection.send_obj(response, None)?;
//                 Ok(())
//             } else {
//                 let mut order = Vec::new();
//
//                 for query in selection {
//                     if query.contains("*") {
//                         for (id, entity) in &sim.entities {
//                             if id == &0 || id == &1 {
//                                 continue;
//                             }
//                             let _query = query.replace("*", &id.to_string());
//                             let addr = Address::from_str(&_query)?;
//                             order.push(addr.clone());
//                             if let Ok(var) = sim.get_var(&addr) {
//                                 data.vars.push(var.clone());
//                             }
//                         }
//                     } else {
//                         // TODO save the ordered list of addresses on the server for handling
//                         // response
//                         let addr = Address::from_str(query)?;
//                         order.push(addr.clone());
//                         if let Ok(var) = sim.get_var(&addr) {
//                             data.vars.push(var.clone());
//                         }
//                     }
//                 }
//
//                 let order_id = client
//                     .order_id_pool
//                     .request_id()
//                     .ok_or(Error::Other("failed getting new order id".to_string()))?;
//                 client.order_store.insert(order_id, order);
//
//                 let response = Message::DataTransferResponse(DataTransferResponse {
//                     data: TransferResponseData::VarOrdered(order_id, data),
//                 });
//                 // client.connection.send_obj(response, None)?;
//                 Ok(())
//             }
//         }
//         _ => Err(Error::Unknown),
//     }
// }

impl Server {
    /// Gets current clock value
    pub async fn get_clock(&mut self) -> Result<usize> {
        if let rpc::worker::Response::Clock(clock) = self
            .worker
            .as_ref()
            .ok_or(Error::WorkerNotConnected("".to_string()))?
            .execute(Signal::from(rpc::worker::Request::Clock))
            .await?
            .into_payload()
        {
            Ok(clock)
        } else {
            unimplemented!()
        }
    }

    // /// Gets currently loaded scenario
    // pub async fn get_scenario(&mut self) -> Result<Scenario> {
    //     if let Response::Scenario(scenario) = self
    //         .worker
    //         .as_ref()
    //         .ok_or(Error::WorkerNotConnected("".to_string()))?
    //         .execute(Request::Scenario)
    //         .await?
    //     {
    //         Ok(scenario)
    //     } else {
    //         unimplemented!()
    //     }
    // }
}
