use std::net::SocketAddr;

use axum::extract::ws::WebSocket;
use axum::extract::{ConnectInfo, Path, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::{any, get};
use axum::{Extension, Form, Json, Router};
use fnv::FnvHashMap;
use tokio_util::sync::CancellationToken;

use crate::net::{CompositeAddress, ConnectionOrAddress, Encoding, Transport};
use crate::rpc::msg::client_server::StatusRequest;
use crate::rpc::msg::{Message, RegisterClientRequest};
use crate::{query, string, Error, Query, QueryProduct, Result, Var};
use crate::{Executor, LocalExec};

type Interface = LocalExec<(ConnectionOrAddress, Message), Result<Message>>;
type InterfaceExt = Extension<Interface>;

pub struct HttpServerHandler {}

pub fn spawn(
    listener_addr: CompositeAddress,
    interface: Interface,
    cancel: CancellationToken,
) -> Result<HttpServerHandler> {
    let sock_addr = if let crate::net::Address::Net(addr) = listener_addr.address {
        addr
    } else {
        return Err(Error::NetworkError(format!(
            "unable to use provided address to spawn an http adapter on server: {}",
            listener_addr
        )));
    };

    let _ = tokio::spawn(async move {
        let router = Router::new()
            .route("/ws", any(ws_handler))
            .route("/status", get(status))
            .route("/register", get(register_client))
            .route("/entities", get(get_entities))
            .route("/world/:entity/:component/:var", get(world_query))
            .layer(Extension(interface));

        log::debug!("listening on {}", listener_addr);
        tokio::select! {
            _ = axum::Server::bind(&sock_addr)
                .serve(router.into_make_service_with_connect_info::<SocketAddr>()) => (),
            _ = cancel.cancelled() => (),
        };
    });

    Ok(HttpServerHandler {})
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(interface): InterfaceExt,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| handle_socket(socket, addr, interface))
}

async fn handle_socket(mut socket: WebSocket, addr: SocketAddr, interface: Interface) {
    while let Some(msg) = socket.recv().await {
        let msg = if let Ok(msg) = msg {
            msg
        } else {
            // client disconnected
            return;
        };

        // println!("http: got websocket message: {:?}", msg);

        if let axum::extract::ws::Message::Binary(bytes) = msg {
            let message: Message = bincode::deserialize(&bytes).unwrap();
            let resp = interface
                .execute((ConnectionOrAddress::Address(addr), message))
                .await
                .unwrap();
            // println!("http: sending back websocket message: {resp:?}");

            if socket
                .send(axum::extract::ws::Message::Binary(
                    bincode::serialize(&resp.unwrap()).unwrap(),
                ))
                .await
                .is_err()
            {
                // client disconnected
                return;
            }
        }
    }
}

async fn get_entities(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(mut interface): InterfaceExt,
) -> Result<impl IntoResponse> {
    let resp = interface
        .execute((
            ConnectionOrAddress::Address(addr),
            Message::EntityListRequest,
        ))
        .await??;
    // match resp {
    //     Message::EntityListResponse(entities) => Ok(Json(entities)),
    //     _ => Err(Error::UnexpectedResponse(format!("{:?}", resp))),
    // }
    Ok(Json(resp))
}

async fn world_query(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path((entity, component, var)): Path<(String, String, String)>,
    Extension(interface): InterfaceExt,
) -> Result<impl IntoResponse> {
    let query = Query::default()
        .description(query::Description::Addressed)
        .filter(query::Filter::Name(vec![string::new_truncate(&entity)]))
        .filter(query::Filter::Component(string::new_truncate(&component)));

    let resp = interface
        .execute((
            ConnectionOrAddress::Address(addr),
            Message::QueryRequest(query),
        ))
        .await??;
    match resp {
        Message::QueryResponse(QueryProduct::AddressedVar(map)) => Ok(Json(
            map.into_iter()
                .map(|(addr, var)| (addr.to_string(), var))
                .collect::<FnvHashMap<String, Var>>(),
        )),
        _ => unimplemented!(),
    }
}

#[derive(Serialize, Deserialize)]
struct ClientInfo {
    pub name: String,
    pub password: Option<String>,
}

async fn register_client(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(interface): InterfaceExt,
    Form(credentials): Form<ClientInfo>,
) -> Result<impl IntoResponse> {
    debug!("http: registering client");
    let result = interface
        .execute((
            ConnectionOrAddress::Address(addr),
            Message::RegisterClientRequest(RegisterClientRequest {
                name: credentials.name,
                // TODO: figure out allowing blocking http clients.
                is_blocking: false,
                auth_token: None,
                encodings: vec![Encoding::Json],
                transports: vec![Transport::HttpServer],
            }),
        ))
        .await?;
    Ok(Json(result))
}

async fn status(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(interface): InterfaceExt,
) -> Result<impl IntoResponse> {
    debug!("http: handling status");
    let result = interface
        .execute((
            ConnectionOrAddress::Address(addr),
            Message::StatusRequest(StatusRequest {
                format: "".to_string(),
            }),
        ))
        .await??;
    Ok(Json(result))
}
