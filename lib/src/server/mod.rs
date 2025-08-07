mod turn;

#[cfg(feature = "grpc_server")]
mod grpc;
#[cfg(feature = "http_server")]
mod http;

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
use crate::util::{decode, encode};
use crate::worker::{WorkerExec, WorkerId};
use crate::{net, rpc, string, worker, Address, Error, EventName, Query, Result};

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
                #[cfg(feature = "quic_transport")]
                Transport::Quic,
                #[cfg(feature = "ws_transport")]
                Transport::WebSocket,
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
/// # Runtime optimizations
///
/// Server capability, in terms of satisfying incoming requests, is determined
/// by it's underlying connection to the cluster.
///
/// Similar to how the inner cluster layer can self-optimize at runtime,
/// moving entities to achieve ever better system performance, the outer layer
/// is capable of runtime optimizations as well.
///
/// Clients can be redirected to different servers based on their interest in
/// particular entities and distance/latency incurred when querying those from
/// a particular server.
pub struct Server {
    pub id: ServerId,

    /// Server configuration
    pub config: Config,

    /// Connection with an entry-point to the underlying simulation system
    pub worker: Option<Worker>,

    /// Map of all clients by their unique identifier.
    pub clients: FnvHashMap<ClientId, Client>,

    pub services: Vec<Service>,

    /// Time of creation of this server
    pub started_at: Instant,

    /// Time since last message received
    last_msg_time: Instant,
    /// Time since last new client connection accepted
    last_accept_time: Instant,

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
    // TODO: return a list of listeners that were successfully established.
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

        // Connect server to worker.
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

/// Spawns a new server using provided config.
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

    // Grpc server can be enabled to serve data to clients through the grpc
    // interface.
    // NOTE: can't hide the stream behind a feature because we must expose
    // it later in the tokio::select! macro.
    let (grpc_client_executor, mut grpc_client_stream, mut grpc_client_stream_multi) =
        LocalExec::new(20);
    #[cfg(feature = "grpc_server")]
    let grpc_listener = config
        .listeners
        .iter()
        .find(|l| l.transport == Some(Transport::GrpcServer));
    #[cfg(feature = "grpc_server")]
    if let Some(listener) = grpc_listener {
        let _ = grpc::spawn(listener.clone(), grpc_client_executor, cancel.clone());
    }

    // Http server can be enabled to serve data through an http endpoint.
    // NOTE: can't hide the stream behind a feature because we must expose
    // it later in the tokio::select! macro.
    let (http_client_executor, mut http_client_stream, _) = LocalExec::new(20);
    // TODO: support multiple http listeners
    #[cfg(feature = "http_server")]
    let http_listener = config
        .listeners
        .iter()
        .find(|l| l.transport == Some(Transport::HttpServer));
    #[cfg(feature = "http_server")]
    if let Some(listener) = http_listener {
        let _ = http::spawn(listener.clone(), http_client_executor, cancel.clone());
    }

    // Server operates multiple listeners, each on different transport.
    // Listeners run on separate tasks.
    // Once direct connection is established, encoding is negotiated.
    net::spawn_listeners(&config.listeners, net_client_executor, cancel.clone())?;

    info!("spawning server task, listeners: {:?}", config.listeners);

    // Create server state.
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
                        let resp = handle_client_message_bytes(caller, msg_bytes, server, None).await;

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
                        let resp = handle_client_message_bytes(caller, msg_bytes, server, Some(s.clone())).await;

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
                Some(((addr, msg), s)) = grpc_client_stream.next() => {
                    debug!("handling grpc client msg: {:?}", msg);
                    let server = server.clone();
                    tokio::spawn(async move {
                        let resp = handle_client_message(addr, msg, server, None).await;
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
                Some(((addr, msg), s)) = grpc_client_stream_multi.next() => {
                    debug!("handling grpc client msg (streaming response): {:?}", msg);
                    let server = server.clone();
                    tokio::spawn(async move {
                        let resp = handle_client_message(addr, msg, server, Some(s.clone())).await;
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
                Some(((addr, msg), s)) = http_client_stream.next() => {
                    let server = server.clone();
                    tokio::spawn(async move {
                        let resp = handle_client_message(addr, msg, server, None).await;
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

/// Controller handler dealing with requests sent through the worker handle.
async fn handle_ctl_request(
    req: rpc::server::Request,
    ctx: Option<rpc::Context>,
    mut server: Arc<Mutex<Server>>,
) -> Result<Signal<rpc::server::Response>> {
    use rpc::server::{Request as ServerRequest, Response as ServerResponse};
    match req {
        ServerRequest::Status => {
            let server = server.lock().await;
            Ok(Signal::new(
                ServerResponse::Status {
                    uptime_secs: server.started_at.elapsed().as_secs(),
                    clients: server.clients.len() as u32,
                },
                ctx,
            ))
        }
        _ => unimplemented!("request: {req:?}"),
    }
}

/// Specialized handler dealing with requests coming from local workers over
/// an in-process channel.
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

async fn handle_client_message(
    caller: ConnectionOrAddress,
    msg: Message,
    server: Arc<Mutex<Server>>,
    resp_stream: Option<mpsc::Sender<Message>>,
) -> Result<Option<Message>> {
    let (encoding, client) = find_client_id(&caller, server.clone()).await?;

    let peer_addr = match caller {
        ConnectionOrAddress::Connection(connection) => None,
        ConnectionOrAddress::Address(socket_addr) => Some(socket_addr),
    };

    let resp = handle_message(
        client.map(|c| c.id),
        peer_addr,
        msg,
        server,
        resp_stream,
        None,
    )
    .await;
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

/// Handler responsible for handling an encoded message from client.
///
/// As part of
async fn handle_client_message_bytes(
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
            // TODO auth incoming clients.

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
                    transport: Transport::Quic,
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

// TODO: consider returning just the client id instead of a tuple.
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

        unimplemented!()
    }

    /// This function handles shutdown cleanup, like stopping spawned services.
    pub fn cleanup(&mut self) -> Result<()> {
        unimplemented!()
    }

    pub fn handle_export_snapshot_request(
        &mut self,
        esr: msg::ExportSnapshotRequest,
        client_id: &ClientId,
    ) -> Result<()> {
        unimplemented!()
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
}

impl Server {
    /// Gets current clock value
    pub async fn get_clock(&mut self) -> Result<usize> {
        let resp = self
            .worker
            .as_ref()
            .ok_or(Error::WorkerNotConnected("".to_string()))?
            .execute(Signal::from(rpc::worker::Request::Clock))
            .await?
            .into_payload();
        if let rpc::worker::Response::Clock(clock) = resp {
            Ok(clock)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }
}
