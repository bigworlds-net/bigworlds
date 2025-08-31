use crate::net::CompositeAddress;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Config {
    pub listeners: Vec<CompositeAddress>,

    pub partition: super::part::Config,

    /// Topology strategy for the worker to follow.
    pub topo_strategy: TopoStrategy,

    /// If ever orphaned, the worker can attempt to bootstrap itself through
    /// spawning a new leader and connecting to it. This effectively "forks"
    /// the simulation state if it was spread across more than this one worker.
    /// In a single-worker situation this serves as an additional mechanism
    /// to keep the worker up and running.
    ///
    /// If this flag is not enabled, the worker, if ever orphaned, will shut
    /// down after the reconnection waiting period is over.
    pub orphan_fork: bool,

    /// During handling of model setting, worker is able to perform a diff and
    /// determine, based on unique name values, what behavior models were
    /// changed with the newly provided model.
    ///
    /// Based on that diff the worker can also "respawn" affected behaviors
    /// to make them effectively follow changes in relevant behavior model.
    ///
    /// Note that this is feature is based on the behavior model names, and as
    /// such it doesn't work at all with "anonymous" behaviors. The behavior
    /// tasks should generally be thought of as opaque by default, with this
    /// feature offering a limited workaround.
    pub behaviors_follow_model_changes: bool,

    /// Flag for disallowing subsequent entity migrations after a successfull
    /// migration.
    pub entity_migration_cooldown_secs: Option<u32>,

    /// Maximum allowed memory use.
    pub max_ram_mb: usize,
    /// Maximum allowed disk use.
    pub max_disk_mb: usize,
    /// Maximum allowed network transfer use.
    pub max_transfer_mb: usize,

    // TODO: overhaul the authentication system.
    /// Whether the worker uses a password to authorize connecting workers.
    pub use_auth: bool,
    /// Password used for incoming connection authorization.
    pub passwd_list: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listeners: vec![],
            partition: Default::default(),
            topo_strategy: Default::default(),
            orphan_fork: true,
            behaviors_follow_model_changes: true,
            entity_migration_cooldown_secs: Some(3),
            max_ram_mb: 0,
            max_disk_mb: 0,
            max_transfer_mb: 0,
            use_auth: false,
            passwd_list: vec![],
        }
    }
}

impl Config {
    /// Convenience constructor setting up a default config with the entity
    /// archive functionality explicitly disabled.
    pub fn no_archive() -> Self {
        Self {
            partition: super::part::Config {
                enable_archive: false,
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

/// Strategy the worker will pursue in terms of connecting directly to other
/// workers.
///
/// # Dynamic adaptation
///
/// All strategies involve worker continuously working to upheld the defined
/// goal. For example connecting to all means also connecting to new workers
/// as they join the cluster. As another example, when the strategy is to only
/// connect to a single remote worker and that remote worker dies, the worker
/// needs to connect to another remote worker.
///
/// # Worker to leader
///
/// Note that the workers' connection to leader part of the network topology
/// is currently fixed and involves leader being connected directly to all
/// workers at all times.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum TopoStrategy {
    /// Maintain connection to only one other worker.
    ///
    /// This works only for a certain number of outlier workers in the cluster,
    /// e.g. relay workers.
    ConnectToOne,
    /// Maintain connection to only two other workers.
    ///
    /// Given all workers in the cluster follow this strategy we should end
    /// up with a ring-like arrangement.
    ConnectToTwo,
    /// Maintain connections with `n` remote workers selected at random.
    ConnectToRandom(u8),
    /// Maintain connections with `n` closest remote workers, selected by ping.
    ConnectToClosest(u8),
    /// Maintain connections to all workers in the cluster.
    #[default]
    ConnectToAll,
}
