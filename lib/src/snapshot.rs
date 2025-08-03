use std::convert::TryFrom;
use std::io::Read;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use fnv::FnvHashMap;
use id_pool::IdPool;

// use crate::time::Instant;
use crate::{EntityId, EntityName, EventName};

use crate::entity::Entity;
use crate::error::{Error, Result};
use crate::{SimHandle, SimModel, SimStarter};

pub trait Snap {
    fn to_snapshot(&self) -> Result<Vec<u8>>;
    fn from_snapshot(bytes: &mut Vec<u8>) -> Result<Self>
    where
        Self: Sized;
}

pub trait SnapPart {
    fn to_snapshot_part(&self) -> Result<Vec<u8>>;
    fn from_snapshot_part(bytes: &[u8], header: SnapshotHeader) -> Result<Self>
    where
        Self: Sized;
}

impl Snap for SimHandle {
    fn to_snapshot(&self) -> Result<Vec<u8>> {
        let header = SnapshotHeader {
            metadata: SnapshotMetadata {
                created: Utc::now(),
                // TODO
                starter: SimStarter::Scenario("".to_string()),
            },
            clock: self.clock,
            model: self.model.clone(),
            entities_idx: self.entity_idx.clone(),
            event_queue: self.event_queue.clone(),
            entity_pool: self.entity_pool.clone(),
        };
        let part = SnapshotPart {
            entities: self.entities.clone(),
        };
        let mut bytes = bincode::serialize(&header).unwrap();
        bytes.extend(bincode::serialize(&part).unwrap());
        Ok(bytes)
    }

    fn from_snapshot(mut bytes: &mut Vec<u8>) -> Result<Self>
    where
        Self: Sized,
    {
        let header = extract_header(&mut bytes)?;
        let part = extract_part(&mut bytes)?;
        Ok(Self {
            model: header.model,
            clock: header.clock,
            event_queue: header.event_queue,
            entities: part.entities,
            entity_idx: header.entities_idx,
            entity_pool: header.entity_pool,
            #[cfg(feature = "machine")]
            machined_entities: Default::default(),
            #[cfg(feature = "machine_dynlib")]
            libs: Default::default(),
        })
    }
}

impl SnapPart for SimHandle {
    fn to_snapshot_part(&self) -> Result<Vec<u8>> {
        let part = SnapshotPart {
            entities: self.entities.clone(),
        };
        let out = bincode::serialize(&part).unwrap();

        // let vec = vec![&self.entities];
        // let out = bincode::serialize(&vec).unwrap();

        Ok(out)
    }

    fn from_snapshot_part(bytes: &[u8], header: SnapshotHeader) -> Result<Self> {
        let part: SnapshotPart = bincode::deserialize(bytes).unwrap();
        let sim = SimHandle {
            model: header.model,
            clock: header.clock,
            event_queue: header.event_queue,
            entities: part.entities,
            entity_idx: header.entities_idx,
            entity_pool: header.entity_pool,
            #[cfg(feature = "machine")]
            machined_entities: Default::default(),
            #[cfg(feature = "machine_dynlib")]
            libs: Default::default(),
        };
        Ok(sim)
    }
}

/// Extracts snapshot header from the provided bytes.
pub fn extract_header(mut bytes: &mut Vec<u8>) -> Result<SnapshotHeader> {
    let mut cursor = &bytes[..];
    let mut header: SnapshotHeader = bincode::deserialize_from(&mut cursor).unwrap();
    *bytes = cursor.to_owned();
    Ok(header)
}

pub fn extract_part(mut bytes: &mut Vec<u8>) -> Result<SnapshotPart> {
    let mut cursor = &bytes[..];
    let mut part: SnapshotPart = bincode::deserialize_from(&mut cursor).unwrap();
    *bytes = cursor.to_owned();
    Ok(part)
}

pub fn prepend_header(mut buf: &mut Vec<u8>, header: SnapshotHeader) -> Result<()> {
    unimplemented!()
}

/// Representation of the simulation state at a certain point in time.
///
/// This representation is not fully self-sufficient, and will require the
/// project file structure for proper initialization.
#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    data: Vec<u8>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SnapshotHeader {
    /// Data about the snapshot itself
    pub metadata: SnapshotMetadata,

    pub clock: usize,
    pub model: SimModel,
    pub entities_idx: FnvHashMap<EntityName, EntityId>,
    pub event_queue: Vec<EventName>,
    pub entity_pool: IdPool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    /// Time when the snapshot was created
    pub created: DateTime<Utc>,
    /// Starter that was used to initialize the simulation
    pub starter: SimStarter,
}

/// Partial snapshot, used when partitioning large snapshots.
// TODO support snapshot partitioning
#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotPart {
    pub entities: FnvHashMap<EntityId, Entity>,
}

impl From<SimHandle> for Snapshot {
    fn from(sim: SimHandle) -> Self {
        unimplemented!()
        // Self {
        //     metadata: SnapshotMetadata {
        //         created: Utc::now(),
        //     },
        //     clock: sim.clock,
        //     event_queue: sim.event_queue,
        //     entity_pool: sim.entity_pool,
        //     model: sim.model,
        //     entities: sim.entities,
        // }
    }
}

impl TryFrom<&Vec<u8>> for Snapshot {
    type Error = Error;
    fn try_from(bytes: &Vec<u8>) -> Result<Self> {
        #[cfg(feature = "lz4")]
        {
            match lz4::block::decompress(&bytes, None) {
                Ok(data) => {
                    let snapshot: Snapshot = bincode::deserialize(&data)
                        .map_err(|e| Error::FailedReadingSnapshot(e.to_string()))?;
                    Ok(snapshot)
                }
                Err(e) => Err(Error::SnapshotDecompressionError(e.to_string())),
            }
        }
        #[cfg(not(feature = "lz4"))]
        {
            let snapshot: Snapshot = bincode::deserialize(&bytes)
                .map_err(|e| Error::FailedReadingSnapshot(e.to_string()))?;
            Ok(snapshot)
        }
    }
}

impl Snapshot {
    pub fn to_bytes(&self, compress: bool) -> Result<Vec<u8>> {
        let mut data: Vec<u8> =
            bincode::serialize(&self).map_err(|e| Error::FailedCreatingSnapshot(e.to_string()))?;
        #[cfg(feature = "lz4")]
        {
            if compress {
                data = lz4::block::compress(&data, None, true)?;
            }
        }
        Ok(data)
    }
}

/// Self-sufficient representation of a simulation state that includes
/// serialized project files.
pub struct Package {
    pub project: FnvHashMap<PathBuf, Vec<u8>>,
    pub snapshot: Snapshot,
}
