use std::collections::HashMap;
use std::fs::File;
use std::future::Future;
use std::io::Write;

use chrono::Utc;
use fjall::KvSeparationOptions;
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
    behavior, rpc, EntityId, EntityName, Error, EventName, Executor, Model, PrefabName, Result, Var,
};

#[cfg(feature = "machine")]
use crate::machine::MachineHandle;

use super::WorkerId;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Config {
    /// Master switch for the automatic entity archiving.
    pub auto_archive_entities: bool,
    /// Number of seconds of no access activity after which entity is archived.
    pub archive_entity_after_secs: u32,

    /// Whether the newly added entities should be added to the archive.
    /// If false new entities will be put into the memory store.
    pub entity_create_to_archive: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            auto_archive_entities: false,
            archive_entity_after_secs: 5,

            entity_create_to_archive: false,
        }
    }
}

/// Definition of a simulation partition held by the worker.
///
/// It contains both data in the form of entities themselves and handles to
/// local logic executing tasks.
pub struct Partition {
    /// Live entities currently stored in memory.
    pub entities: FnvHashMap<EntityName, Entity>,

    /// FS-backed entity archive.
    ///
    /// Based on chosen policy, entities can be moved into the archive based
    /// on observed access patterns.
    pub archive: fjall::Partition,

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

    pub config: Config,
}

impl Partition {
    pub fn new(
        behavior_exec: LocalExec<
            Signal<rpc::worker::Request>,
            Result<Signal<rpc::worker::Response>>,
        >,
        config: Config,
    ) -> Result<Self> {
        Ok(Self {
            entities: FnvHashMap::default(),
            archive: fjall::Config::new("workerpart")
                .max_write_buffer_size(1024 * 1024 * 1024)
                .max_journaling_size(2 * 1024 * 1024 * 1024)
                // .fsync_ms(Some(1000))
                .open()?
                .open_partition(
                    "entities",
                    fjall::PartitionCreateOptions::default().max_memtable_size(128 * 1024 * 1024),
                )?,
            #[cfg(feature = "machine")]
            machines: Default::default(),
            behaviors: FnvHashMap::default(),
            behavior_broadcast: tokio::sync::broadcast::channel(20),
            behavior_exec,
            #[cfg(feature = "behavior_dynlib")]
            libs: vec![],
            config,
        })
    }

    /// Initializes a worker partition using the given model.
    pub async fn from_model(
        model: &Model,
        behavior_exec: LocalExec<
            Signal<rpc::worker::Request>,
            Result<Signal<rpc::worker::Response>>,
        >,
        config: Config,
    ) -> Result<Self> {
        // Create the new partition object.
        let mut part = Partition::new(behavior_exec.clone(), config)?;

        // Initialize non-entity-bound behaviors.
        behavior::spawn_non_entity_bound(model, &mut part, behavior_exec);

        Ok(part)
    }
}

impl Partition {
    /// Returns entity by name.
    pub fn get_entity(&mut self, name: &EntityName) -> Result<&crate::entity::Entity> {
        if let Some(entity) = self.entities.get_mut(name) {
            entity.meta.last_access = Some(Utc::now().timestamp() as u32);
            Ok(entity)
        } else {
            Err(Error::FailedGettingEntityByName(name.to_owned()))
        }
    }

    pub fn get_entity_archived(&self, name: &EntityName) -> Result<crate::entity::Entity> {
        if let Ok(Some(slice)) = self.archive.get(name) {
            #[cfg(feature = "archive")]
            unsafe {
                let entity_arch = rkyv::access_unchecked::<crate::entity::ArchivedEntity>(&slice);
                let entity = rkyv::deserialize::<Entity, rkyv::rancor::Error>(entity_arch).unwrap();
                Ok(entity)
            }
            #[cfg(not(feature = "archive"))]
            {
                let entity: crate::entity::Entity = bincode::deserialize(&slice)?;
                Ok(entity)
            }
        } else {
            Err(Error::FailedGettingEntityByName(name.to_owned()))
        }
    }

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
        if self.config.entity_create_to_archive {
            #[cfg(feature = "archive")]
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&entity).unwrap();

            #[cfg(not(feature = "archive"))]
            let bytes = bincode::serialize(&entity)?;

            self.archive.insert(name, bytes.as_slice()).unwrap();
        } else {
            self.entities.insert(name, entity);
        }

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

        // Check if the entity is present in memory or if it's currently in
        // cold storage.
        if self.entities.remove(&name).is_some() {
            Ok(())
        } else if self.archive.remove(&name).is_ok() {
            Ok(())
        } else {
            Err(Error::FailedGettingEntityByName(name))
        }
    }

    // pub fn get_entity_mut(&mut self, name: &EntityName) -> Result<&mut Entity> {
    //     if let Some(entity) = self.entities.get_mut(name) {
    //         return Ok(entity);
    //     } else {
    //         // Pull the entity from archive into memory.
    //     }

    //     if let Some(entity_bytes) = self.archive.get(name)? {
    //         // let entity_access =
    //     }
    // }

    /// Put long-unaccessed entities into cold-storage.
    pub fn archive_entities(&mut self) -> Result<usize> {
        if !self.config.auto_archive_entities {
            return Ok(0);
        }

        let mut to_archive = vec![];
        for (name, entity) in &self.entities {
            if let Some(last_access) = entity.meta.last_access {
                if chrono::Utc::now().timestamp() as u32 - last_access
                    > self.config.archive_entity_after_secs
                {
                    to_archive.push(name.to_owned());
                }
            } else {
                // TODO: if the entity was never accessed archive it immediately.
                // to_archive.push(name.to_owned());
            }
        }
        let len = to_archive.len();
        for entity_name in to_archive {
            if let Some(entity) = self.entities.remove(&entity_name) {
                #[cfg(feature = "archive")]
                let entity_serialized = rkyv::to_bytes::<rkyv::rancor::Error>(&entity)
                    .map_err(|e| Error::RkyvError(e.to_string()))?;
                #[cfg(not(feature = "archive"))]
                let entity_serialized = bincode::serialize(&entity)?;
                self.archive
                    .insert(entity_name, entity_serialized.as_slice())?;
            }
        }

        Ok(len)
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
        unimplemented!()
        // let mut entity_data = bincode::serialize(&self.entities)?;

        // #[cfg(feature = "lz4")]
        // {
        //     if compress {
        //         entity_data = lz4::block::compress(&data, None, true)?;
        //     }
        // }

        // Ok(entity_data)
    }
}
