mod process;

pub use process::process_query;

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use fnv::FnvHashMap;

use crate::address::LocalAddress;
use crate::entity::Entity;
use crate::time::Instant;
use crate::{
    Address, CompName, EntityId, EntityName, EventName, Float, Int, Result, Var, VarName, VarType,
};

/// Collection of items defining an individual query.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Query {
    pub description: Description,
    pub layout: Layout,
    pub filters: Vec<Filter>,
    pub mappings: Vec<Map>,
    pub scope: Scope,
    pub archive: Archive,
    pub limits: Limits,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Limits {
    pub entities: Option<u32>,
    pub product_entries: Option<u32>,
    pub size_bytes: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Archive {
    Only,
    #[default]
    Ignore,
    Include,
    IncludeAndRestoreSelected,
}

impl Query {
    pub fn scope(mut self, scope: Scope) -> Self {
        self.scope = scope;
        self
    }

    pub fn description(mut self, description: Description) -> Self {
        self.description = description;
        self
    }

    pub fn filter(mut self, filter: Filter) -> Self {
        self.filters.push(filter);
        self
    }

    pub fn map(mut self, map: Map) -> Self {
        self.mappings.push(map);
        self
    }

    pub fn archive(mut self, map: Archive) -> Self {
        self.archive = map;
        self
    }

    pub fn limits(mut self, entities: Option<u32>, entries: Option<u32>) -> Self {
        self.limits.entities = entities;
        self.limits.product_entries = entries;
        self
    }
}

impl Default for Query {
    fn default() -> Self {
        Self {
            description: Description::default(),
            layout: Layout::default(),
            /// By default all entities are selected, there are no filters.
            filters: vec![],
            /// By default no variables are included in the response.
            mappings: vec![],
            scope: Scope::default(),
            archive: Archive::default(),
            // By default there's no limits on the response.
            limits: Limits::default(),
        }
    }
}

/// Uniform query product type.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum QueryProduct {
    NativeAddressedVar(FnvHashMap<(EntityName, CompName, VarName), Var>),
    AddressedVar(FnvHashMap<Address, Var>),
    NameAddressedVar(FnvHashMap<EntityName, Var>),
    NameAddressedVars(FnvHashMap<EntityName, Vec<Var>>),
    AddressedTyped(AddressedTypedMap),
    OrderedVar(u32, Vec<Var>),
    Var(Vec<Var>),
    Empty,
}

impl QueryProduct {
    pub fn merge(&mut self, other: QueryProduct) -> Result<()> {
        match self {
            QueryProduct::NativeAddressedVar(hash_map) => todo!(),
            QueryProduct::NameAddressedVar(map) => match other {
                QueryProduct::NameAddressedVar(_map) => map.extend(_map),
                _ => todo!(),
            },
            QueryProduct::NameAddressedVars(map) => match other {
                QueryProduct::NameAddressedVars(_map) => map.extend(_map),
                _ => todo!(),
            },
            QueryProduct::AddressedVar(map) => match other {
                QueryProduct::AddressedVar(_map) => map.extend(_map),
                QueryProduct::Var(vec) => unimplemented!(),
                _ => todo!(),
            },
            QueryProduct::AddressedTyped(addressed_typed_map) => todo!(),
            QueryProduct::OrderedVar(_, vec) => todo!(),
            QueryProduct::Var(ref mut vec) => match other {
                QueryProduct::NativeAddressedVar(hash_map) => todo!(),
                QueryProduct::NameAddressedVar(hash_map) => todo!(),
                QueryProduct::NameAddressedVars(hash_map) => todo!(),
                QueryProduct::AddressedVar(hash_map) => todo!(),
                QueryProduct::AddressedTyped(addressed_typed_map) => todo!(),
                QueryProduct::OrderedVar(_, vec) => todo!(),
                QueryProduct::Var(_vec) => {
                    vec.extend(_vec);
                }
                QueryProduct::Empty => todo!(),
            },
            QueryProduct::Empty => *self = other,
        };

        Ok(())
    }

    pub fn to_vec(self) -> Vec<Var> {
        match self {
            QueryProduct::AddressedVar(map) => map.values().cloned().collect::<Vec<_>>(),
            QueryProduct::Var(vec) => vec,
            _ => unimplemented!(),
        }
    }

    pub fn to_map(self) -> Result<FnvHashMap<Address, Var>> {
        match self {
            QueryProduct::AddressedVar(map) => Ok(map),
            QueryProduct::Var(vec) => Err(crate::Error::FailedConversion(
                "Can't make query product into a map, lacking address information".to_string(),
            )),
            _ => unimplemented!(),
        }
    }

    pub fn to_named_map(self) -> Result<FnvHashMap<EntityName, Var>> {
        match self {
            QueryProduct::NameAddressedVar(map) => Ok(map),
            QueryProduct::Var(vec) => Err(crate::Error::FailedConversion(
                "Can't make query product into a map, lacking address information".to_string(),
            )),
            _ => unimplemented!(),
        }
    }

    pub fn to_named_map_many(self) -> Result<FnvHashMap<EntityName, Vec<Var>>> {
        match self {
            QueryProduct::NameAddressedVar(map) => Ok(map
                .into_iter()
                .map(|(name, var)| (name, vec![var]))
                .collect()),
            QueryProduct::NameAddressedVars(map) => Ok(map),
            QueryProduct::Var(vec) => Err(crate::Error::FailedConversion(
                "Can't make query product into a map, lacking address information".to_string(),
            )),
            _ => unimplemented!(),
        }
    }

    pub fn to_var(self) -> Result<Var> {
        match self {
            QueryProduct::Var(mut vec) => Ok(vec.pop().ok_or(crate::Error::FailedConversion(
                "Can't make query product into a single var, no vars available".to_string(),
            ))?),
            _ => unimplemented!(),
        }
    }
}

/// Defines possible trigger conditions for a query.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Trigger {
    /// Trigger on each simulation step.
    #[default]
    StepEvent,
    /// Trigger each time specific event is fired.
    Event(EventName),
    /// Trigger each time any of the specified events is fired.
    EventAny(Vec<EventName>),

    /// Trigger each time the given time elapses.
    Timer(std::time::Duration),

    /// Storage on the specified entity was accessed.
    EntityAccess(EntityName),
    /// Storage on the specified entity was changed.
    EntityMutation(EntityName),

    /// Data at the given address was changed.
    DataMutation(Address),
    /// Data at any of the given addresses was changed.
    DataMutationAny(Vec<Address>),
    /// Data at all of the given addresses was changed.
    DataMutationAll(Vec<Address>),
}

/// Defines all possible entity filtering mechanisms when processing a query.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Filter {
    /// Select entities that have all the specified components.
    AllComponents(Vec<CompName>),
    /// Select entities that have one or more of specified components.
    SomeComponents(Vec<CompName>),
    /// Select entities that have the specified component.
    Component(CompName),
    /// Select entities that match any of the provided names.
    Name(Vec<EntityName>),
    /// Filter by entity id.
    Id(Vec<EntityId>),
    /// Filter by some variable being in specified range.
    VarRange(LocalAddress, Var, Var),
    /// Filter by some variable being in specified range.
    AttrRange(String, Var, Var),
    /// Filter by entity distance to some point, matching on the position
    /// component (x, y and z coordinates, then x,y and z max distance).
    // TODO use single address to vector3 value.
    Distance(Address, Address, Address, Float, Float, Float),
    /// Filter by entity distance to any of multiple points.
    DistanceMultiPoint(Vec<(Address, Address, Address, Float, Float, Float)>),
    /// Select entities based on where they are currently stored.
    Node(NodeFilter),
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum NodeFilter {
    Local(Option<u32>),
    Remote(Option<u32>),
}

/// Defines all possible ways to map entity data, allowing for a fine-grained
/// control over the resulting query product.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Map {
    /// Map all the data stored on selected entities.
    All,
    /// Select data based on address string matching.
    SelectAddrs(Vec<Address>),
    /// Select data bound to selected components.
    Components(Vec<CompName>),
    /// Select data bound to a single component.
    Component(CompName),

    Var(VarName),
    VarType(VarType),
}

impl Map {
    pub fn components(components: Vec<&str>) -> Self {
        let mut c = Vec::<String>::new();
        for comp in components.into_iter() {
            c.push(comp.to_owned());
        }
        Map::Components(c)
    }
}

/// Defines different ways of structuring the query product in terms of
/// associating data points with their addresses.
// TODO: research how much of fine-grained control is necessary here.
#[derive(Copy, Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Description {
    /// Describe values with an address tuple.
    NativeDescribed,
    /// Describe values with an address structure.
    Addressed,
    /// Describe values with with an entity name.
    Entity,
    /// Describe multiple values with a single entity name.
    EntityMany,
    /// Describe values with with a component name.
    Component,
    /// Describe values with a variable name.
    Var,
    /// Describe values with a (component, var) tuple.
    ComponentVar,
    /// Values ordered based on an order table.
    Ordered,
    #[default]
    None,
}

/// Defines possible layout versions for the returned data.
#[derive(Copy, Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Layout {
    /// Use the internal value representation type built on Rust's enum.
    #[default]
    Var,
    /// Coerce all values to strings.
    String,
    /// Use a separate map/list for each variable type.
    Typed,
}

/// Defines how widely should the search be spread across the cluster.
///
/// Effectively it's a way to potentially limit the number of workers a query
/// should be performed on.
#[derive(Copy, Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Scope {
    /// Broadcast the query across the whole cluster.
    #[default]
    Global,
    /// Restrict the query to workers running on the same node.
    LocalNode,
    /// Restrict the query to the worker that originally received the query.
    LocalWorker,
    /// Number of times the query can be "broadcasted" to all visible workers.
    Broadcasts(u8),
    /// Number of times the query will be passed on to another worker.
    ///
    /// # Note
    ///
    /// If the number is smaller than the number of currently visible workers,
    /// a random selection of workers is used.
    Edges(u8),
    /// Number of workers to receive the query.
    Workers(u8),
    /// Maximum roundtrip time allowed when considering broadcasting query to
    /// remote workers.
    ///
    /// # Note
    ///
    /// This is a best-effort scope requirement performed based on available
    /// connection metrics data, which can be limited.
    Time(std::time::Duration),
}

/// Combines multiple products.
// TODO: expand beyond only similarly described products.
pub fn combine_products(mut products: Vec<QueryProduct>) -> QueryProduct {
    let mut final_product = match products.pop() {
        Some(p) => p,
        None => return QueryProduct::Empty,
    };

    for product in products {
        match &mut final_product {
            QueryProduct::AddressedVar(map) => match product.into() {
                QueryProduct::AddressedVar(_map) => {
                    for (k, v) in _map {
                        if !map.contains_key(&k) {
                            map.insert(k, v);
                        }
                    }
                }
                _ => (),
            },
            _ => (),
        }
    }

    final_product
}

#[derive(Default, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct AddressedTypedMap {
    pub strings: FnvHashMap<Address, String>,
    pub ints: FnvHashMap<Address, Int>,
    pub floats: FnvHashMap<Address, Float>,
    pub bools: FnvHashMap<Address, bool>,
}
