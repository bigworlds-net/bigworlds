use std::time::Duration;

use crate::net::{CompositeAddress, Encoding, Transport};

/// Configuration settings for server.
#[derive(Clone)]
pub struct Config {
    pub listeners: Vec<CompositeAddress>,
    /// Name of the server.
    pub name: String,
    /// Description of the server.
    pub description: String,

    /// Time since last traffic from any client until server is shutdown,
    /// set to none to keep alive forever.
    pub self_keepalive: Option<Duration>,
    /// Time between polls in the main loop.
    pub poll_wait: Duration,
    /// Delay between polling for new incoming client connections.
    pub accept_delay: Duration,

    /// Time since last traffic from client until connection is terminated.
    pub client_keepalive: Option<Duration>,
    /// Compress outgoing messages
    pub use_compression: bool,

    /// Whether to require authorization of incoming clients.
    pub require_auth: bool,
    /// User and password pairs for client authorization.
    pub auth_pairs: Vec<(String, String)>,

    // TODO: additional configuration specific to the http adapter.
    pub http: HttpConfig,

    pub cache: CacheConfig,

    /// List of transports supported for client connections.
    pub transports: Vec<Transport>,
    /// List of encodings supported for client connections.
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

            require_auth: false,
            auth_pairs: Vec::new(),

            http: HttpConfig::default(),
            cache: CacheConfig::default(),

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

#[derive(Clone)]
pub struct CacheConfig {
    pub enabled: bool,
    pub entry_ttl_ms: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            entry_ttl_ms: 2,
        }
    }
}
