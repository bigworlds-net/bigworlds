use fnv::FnvHashMap;

use crate::net::CompositeAddress;

use super::Permissions;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub listeners: Vec<CompositeAddress>,

    pub single_threaded: bool,
    pub max_memory: usize,
    pub workers_per_thread: u16,

    pub acl: FnvHashMap<String, Permissions>,
}

impl Default for Config {
    fn default() -> Self {
        let mut acl = FnvHashMap::default();
        acl.insert("root".to_string(), Permissions::root());
        Self {
            listeners: vec![],
            single_threaded: false,
            max_memory: 1000,
            workers_per_thread: 2,
            acl,
        }
    }
}
