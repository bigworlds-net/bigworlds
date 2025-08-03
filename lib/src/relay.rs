use fnv::FnvHashMap;

use crate::{Address, Model, Var};

#[derive(Clone, Copy)]
pub enum CacheConfig {
    Off,
    /// Only cache data from the current step
    CurrentStepOnly,
    /// Cache data from up to a few steps back
    LastSteps(u8),
}

/// Stateless cluster participant tasked with forwarding signals.
///
/// # Relay-backed server
///
/// Relay instance can be used to back a server that will respond to client
/// queries. Relay servers allow for spreading the load of serving connected
/// clients across more machines.
///
/// Relay-backed server is able to introduce some optimizations when dealing
/// with data queries from clients, instead of simply forwarding them "as is".
/// For example it can make use of delta encoding and local caching.
pub struct Relay {
    pub step: usize,
    pub model: Model,

    pub net: RelayNet,

    pub cache_config: CacheConfig,
    pub cache: Vec<FnvHashMap<Address, Var>>,
}

pub struct RelayNet {}
