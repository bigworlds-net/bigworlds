use fnv::FnvHashMap;

use crate::{Query, QueryProduct};

/// Server-side cache.
#[derive(Default)]
pub struct Cache {
    pub queries: FnvHashMap<Query, QueryProduct>,
}
