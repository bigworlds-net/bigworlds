use std::fmt::{Display, Formatter};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::str::FromStr;

use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::executor::{Executor, LocalExec};
use crate::{Error, Result};

pub mod framed_tcp;
#[cfg(feature = "quic_transport")]
pub mod quic;
#[cfg(feature = "ws_transport")]
pub mod ws;
#[cfg(feature = "zmq_transport")]
pub mod zmq;

/// Unified representation of an encoding, transport and address triple.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CompositeAddress {
    pub encoding: Option<Encoding>,
    pub transport: Option<Transport>,
    pub address: Address,
}

impl FromStr for CompositeAddress {
    type Err = Error;
    fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
        if s.contains("://") {
            let split = s.split("://").collect::<Vec<&str>>();
            if split[0].contains("@") {
                let _split = split[0].split("@").collect::<Vec<&str>>();
                Ok(CompositeAddress {
                    encoding: Some(Encoding::from_str(_split[0])?),
                    transport: Some(Transport::from_str(_split[1])?),
                    address: split[1].parse()?,
                })
            } else {
                Ok(CompositeAddress {
                    encoding: None,
                    transport: Some(Transport::from_str(split[0])?),
                    address: split[1].parse()?,
                })
            }
        } else if s.contains("@") {
            let split = s.split("@").collect::<Vec<&str>>();
            Ok(CompositeAddress {
                encoding: Some(Encoding::from_str(split[0])?),
                transport: None,
                address: split[1].parse()?,
            })
        } else {
            Ok(CompositeAddress {
                encoding: None,
                transport: None,
                address: s.parse()?,
            })
        }
    }
}

impl Display for CompositeAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut out = self.address.to_string();
        if let Some(transport) = self.transport {
            out = format!("{}://{}", transport.to_string(), out);
        }
        if let Some(encoding) = self.encoding {
            out = format!("{}@{}", encoding.to_string(), out);
        }
        write!(f, "{}", out)
    }
}

impl Default for CompositeAddress {
    fn default() -> Self {
        Self {
            encoding: None,
            transport: None,
            address: Address::default(),
        }
    }
}

impl CompositeAddress {
    pub fn available() -> Result<Self> {
        Ok(Self {
            encoding: None,
            transport: None,
            address: Address::Net(get_available_address()?),
        })
    }

    pub fn normalize_non_routable(self) -> Result<Self> {
        let socket_addr: SocketAddr = self.address.clone().try_into().unwrap();
        if socket_addr.ip() == IpAddr::from_str("0.0.0.0").unwrap() {
            let port = socket_addr.port();
            Ok(CompositeAddress::from_str(&format!("127.0.0.1:{}", port))?)
        } else {
            Ok(self)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum Address {
    Net(SocketAddr),
    NetUrl(Url),
    /// Addressing scheme for file-based transports such as unix domain
    /// sockets.
    File(String),
}

impl FromStr for Address {
    type Err = Error;
    fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
        if let Ok(mut socket_addrs) = s.to_socket_addrs() {
            Ok(Self::Net(socket_addrs.next().unwrap()))
        } else if let Ok(url) = s.parse::<Url>() {
            Ok(Self::NetUrl(url))
        } else {
            Ok(Self::File(s.to_string()))
        }
    }
}

impl TryInto<SocketAddr> for Address {
    type Error = Error;
    fn try_into(self) -> core::result::Result<SocketAddr, Self::Error> {
        match self {
            Address::Net(net) => Ok(net),
            _ => Err(Error::InvalidAddress(format!(
                "unable to turn abstract address into socket address {:?}",
                self
            ))),
        }
    }
}

impl Display for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Net(sock) => write!(f, "{}", sock.to_string()),
            Self::NetUrl(url) => write!(f, "{}", url.to_string()),
            Self::File(path) => write!(f, "{}", path),
        }
    }
}

impl Default for Address {
    fn default() -> Self {
        Self::Net(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9090))
    }
}

/// Creates an easily bindable address using the `0.0.0.0` meta-address and
/// any available port.
pub fn get_available_address() -> Result<SocketAddr> {
    let listener = std::net::TcpListener::bind("0.0.0.0:0")?;
    let addr = listener.local_addr()?;
    Ok(addr)
}

/// List of possible network transports.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub enum Transport {
    /// Framed TCP using tokio socket, varint frame description.
    FramedTcp,
    /// UDP-based Quic transport, supports unordered and/or unreliable.
    Quic,
    /// WebSocket transport aimed at browser connections.
    WebSocket,
    /// Secure WebSocket transport aimed at browser connections.
    SecureWebSocket,
    /// TCP using ZeroMQ.
    ZmqTcp,
    /// IPC using ZeroMQ.
    ZmqIpc,
    /// IPC using NNG.
    NngIpc,
    /// WebSocket using NNG.
    NngWs,

    /// While HTTP is not really a transport, we define it as one here so we
    /// can use it with composite addresses, e.g. `http://127.0.0.1:8000`.
    /// This is useful when spawning a multi-transport server for example.
    Http,
}

impl Display for Transport {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FramedTcp => write!(f, "tcp"),
            Self::Quic => write!(f, "quic"),
            Self::WebSocket => write!(f, "ws"),
            Self::SecureWebSocket => write!(f, "wss"),
            Self::Http => write!(f, "http"),
            Self::ZmqTcp => write!(f, "zmq_tcp"),
            Self::ZmqIpc => write!(f, "zmq_ipc"),
            Self::NngIpc => write!(f, "nng_ipc"),
            Self::NngWs => write!(f, "nng_ws"),
        }
    }
}

impl FromStr for Transport {
    type Err = Error;
    fn from_str(s: &str) -> core::result::Result<Self, Error> {
        match s.to_lowercase().as_str() {
            "tcp" => Ok(Transport::FramedTcp),
            "quic" => Ok(Transport::Quic),
            "websocket" | "web_socket" | "web-socket" | "ws" => return Ok(Transport::WebSocket),
            "wss" => return Ok(Transport::SecureWebSocket),
            "http" => Ok(Transport::Http),
            "zmq_tcp" | "zmq" | "zeromq" => return Ok(Transport::ZmqTcp),
            "zmq_ipc" | "ipc" => return Ok(Transport::ZmqIpc),
            _ => {
                return Err(Error::ParsingError(format!(
                    "failed parsing transport from string: {}, available transports: {:?}",
                    s,
                    Transport::list_supported()
                )))
            }
        }
    }
}

impl Transport {
    /// Lists all supported transports.
    pub fn list_supported() -> Vec<String> {
        let mut list = vec![];
        list.push(Transport::FramedTcp.to_string());
        #[cfg(feature = "quic_transport")]
        list.push(Transport::Quic.to_string());
        #[cfg(feature = "ws_transport")]
        list.push(Transport::WebSocket.to_string());
        #[cfg(feature = "zmq_transport")]
        {
            list.push(Transport::ZmqTcp.to_string());
            list.push(Transport::ZmqIpc.to_string());
        }
        #[cfg(feature = "nng_transport")]
        {
            list.push(Transport::NngWs.to_string());
            list.push(Transport::NngWs.to_string());
        }
        list
    }
}

/// List of possible formats for encoding data sent over the network.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub enum Encoding {
    /// Fast binary format for communication between Rust apps.
    Bincode,
    /// Binary format with implementations in many different languages.
    MsgPack,
    /// Very common but more verbose format.
    #[default]
    Json,
}

impl FromStr for Encoding {
    type Err = Error;
    fn from_str(s: &str) -> core::result::Result<Self, Error> {
        let e = match s.to_lowercase().as_str() {
            "bincode" | "bin" => Self::Bincode,
            #[cfg(feature = "msgpack_encoding")]
            "msgpack" | "messagepack" | "rmp" => Self::MsgPack,
            #[cfg(feature = "json_encoding")]
            "json" => Self::Json,
            _ => {
                return Err(Error::Other(format!(
                    "failed parsing encoding from string: {}",
                    s
                )))
            }
        };
        Ok(e)
    }
}

impl Display for Encoding {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bincode => write!(f, "bincode"),
            Self::MsgPack => write!(f, "msgpack"),
            Self::Json => write!(f, "json"),
        }
    }
}

#[derive(Clone, strum::Display)]
pub enum ConnectionOrAddress {
    Connection(quinn::Connection),
    Address(SocketAddr),
}

// TODO: For extra compatibility, as well as access to more exotic transports
// such UNIX domain sockets, we can add additional listening solutions with
// their own setups. E.g. zmq, nng, laminar
pub fn spawn_listeners(
    listener_addrs: &Vec<CompositeAddress>,
    net_exec: LocalExec<(ConnectionOrAddress, Vec<u8>), Vec<u8>>,
    cancel: CancellationToken,
) -> Result<()> {
    for listener_addr in listener_addrs {
        match listener_addr.transport {
            None | Some(Transport::Quic) => quic::spawn(
                listener_addr.address.clone().try_into()?,
                net_exec.clone(),
                cancel.clone(),
            )?,
            Some(Transport::FramedTcp) => framed_tcp::spawn_listener(
                listener_addr.address.clone().try_into()?,
                net_exec.clone(),
                cancel.clone(),
            ),
            #[cfg(feature = "ws_transport")]
            Some(Transport::WebSocket) => ws::spawn_listener(
                listener_addr.address.clone().try_into()?,
                net_exec.clone(),
                cancel.clone(),
            ),
            #[cfg(feature = "http_server")]
            Some(Transport::Http) => {
                // We spawn the http listener elsewhere
                continue;
            }
            _ => unimplemented!(),
        };
        trace!(
            "listener task spawned: encoding: {:?}, transport: {:?}, address: {:?}",
            listener_addr.encoding,
            listener_addr.transport,
            listener_addr.address
        );
    }

    Ok(())
}
