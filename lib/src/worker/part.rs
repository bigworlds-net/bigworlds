use std::collections::HashMap;
use std::fs::File;
use std::future::Future;
use std::io::Write;

use fnv::FnvHashMap;
use futures::future::BoxFuture;
use tokio::sync::watch;
use tokio_stream::StreamExt;

use crate::address::LocalAddress;
use crate::behavior::{BehaviorHandle, BehaviorTarget};
use crate::entity::Entity;
use crate::executor::{LocalExec, Signal};
use crate::model::behavior::BehaviorInner;
use crate::model::{self, EventModel, PrefabModel};
use crate::{
    behavior, rpc, EntityId, EntityName, Error, EventName, Executor, Model, PrefabName, Result,
    StringId, Var,
};

#[cfg(feature = "machine")]
use crate::machine::MachineHandle;

use super::WorkerId;

/// Definition of a simulation partition held by the worker.
///
/// It contains both data in the form of entities themselves and handles to
/// local logic executing tasks.
pub struct Partition {
    /// Map of live entities.
    pub entities: FnvHashMap<EntityName, Entity>,

    /// Handles to synced behaviors.
    pub behaviors: FnvHashMap<BehaviorTarget, Vec<BehaviorHandle>>,
    /// Broadcast sender for communicating with unsynced behaviors.
    pub behavior_broadcast: (
        tokio::sync::broadcast::Sender<rpc::behavior::Request>,
        tokio::sync::broadcast::Receiver<rpc::behavior::Request>,
    ),

    /// Handles to machine behaviors.
    #[cfg(feature = "machine")]
    pub machines: Vec<MachineHandle>,

    /// Executor for handling behavior-originating requests to the worker is
    /// stored on the partition level so we can later easily pass it when
    /// spawning behaviors.
    pub behavior_exec:
        LocalExec<Signal<rpc::worker::Request>, Result<Signal<rpc::worker::Response>>>,

    /// Currently loaded dynamic libraries.
    #[cfg(feature = "behavior_dynlib")]
    pub libs: Vec<libloading::Library>,
}

impl Partition {
    pub fn new(
        behavior_exec: LocalExec<
            Signal<rpc::worker::Request>,
            Result<Signal<rpc::worker::Response>>,
        >,
    ) -> Self {
        Self {
            entities: FnvHashMap::default(),
            #[cfg(feature = "machine")]
            machines: Default::default(),
            behaviors: FnvHashMap::default(),
            behavior_broadcast: tokio::sync::broadcast::channel(20),
            behavior_exec,
            #[cfg(feature = "behavior_dynlib")]
            libs: vec![],
        }
    }

    /// Initializes a worker partition using the given model.
    pub async fn from_model(
        model: &Model,
        behavior_exec: LocalExec<
            Signal<rpc::worker::Request>,
            Result<Signal<rpc::worker::Response>>,
        >,
    ) -> Result<Self> {
        // Construct the partition.
        let mut part = Partition {
            entities: FnvHashMap::default(),
            #[cfg(feature = "machine")]
            machines: Default::default(),
            behaviors: FnvHashMap::default(),
            behavior_broadcast: tokio::sync::broadcast::channel(20),
            behavior_exec: behavior_exec.clone(),
            #[cfg(feature = "behavior_dynlib")]
            libs: Vec::new(),
        };

        // Initialize non-entity-bound behaviors.
        behavior::spawn_non_entity_bound(model, &mut part, behavior_exec);

        Ok(part)
    }
}

impl Partition {
    /// Adds an already constructed entity onto the partition.
    pub fn add_entity(&mut self, name: EntityName, entity: Entity, model: &Model) -> Result<()> {
        // Find applicable behaviors and spawn them.
        for behavior in model.behaviors.iter().filter(|b| {
            b.targets.iter().any(|target| match target {
                model::behavior::InstancingTarget::PerEntityWithAllComponents(comps) => {
                    comps.iter().all(|comp| entity.components.contains(comp))
                }
                model::behavior::InstancingTarget::PerEntityWithAnyComponent(comps) => {
                    comps.iter().any(|comp| entity.components.contains(comp))
                }
                _ => false,
            })
        }) {
            behavior::spawn_onto_partition(
                behavior,
                self,
                BehaviorTarget::Entity(name.clone()),
                self.behavior_exec.clone(),
            )?;
        }

        // Insert the entity into the partition.
        self.entities.insert(name, entity);

        Ok(())
    }

    /// Spawns a new entity from a prefab and adds it onto the partition.
    pub fn spawn_entity(
        &mut self,
        name: EntityName,
        prefab: &Option<PrefabName>,
        data: &FnvHashMap<LocalAddress, Var>,
        model: &Model,
    ) -> Result<()> {
        // Create the entity object, potentially using the prefab information.
        let mut entity = if let Some(prefab) = prefab {
            Entity::from_prefab_name(prefab, &model)?
        } else {
            Entity::empty()
        };

        // Insert initial data if available.
        for (addr, var) in data {
            entity.storage.set_from_var(&addr, var.clone())?;
        }

        self.add_entity(name, entity, model)?;

        Ok(())
    }

    /// Removes an entity from the partition by name.
    ///
    /// Returns an error if the entity with the provided name doesn't exist.
    pub fn remove_entity(&mut self, name: EntityName) -> Result<()> {
        // Retain behaviors that are not associated with the entity we're
        // removing.
        self.behaviors.retain(|target, _| match target {
            BehaviorTarget::Entity(name_) => &name != name_,
            _ => true,
        });
        if self.entities.remove(&name).is_some() {
            Ok(())
        } else {
            Err(Error::FailedGettingEntityByName(name))
        }
    }

    /// Serialize partition's entity data to a vector of bytes.
    ///
    /// # Behaviors
    ///
    /// Running behavior tasks are not persisted. Any state they might hold
    /// can be lost at any time unless they save it out to entity storage.
    ///
    /// # Compression
    ///
    /// Optional compression using zstd can be performed.
    pub fn to_snapshot(&self, name: &str, compress: bool) -> Result<Vec<u8>> {
        let mut entity_data = bincode::serialize(&self.entities)?;

        #[cfg(feature = "lz4")]
        {
            if compress {
                entity_data = lz4::block::compress(&data, None, true)?;
            }
        }

        Ok(entity_data)
    }
}
