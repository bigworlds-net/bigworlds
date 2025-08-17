use std::convert::TryFrom;
use std::io::Read;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use fnv::FnvHashMap;
use id_pool::IdPool;

use crate::{EntityId, EntityName, EventName, Model};

use crate::entity::Entity;
use crate::error::{Error, Result};
use crate::SimHandle;

pub const SNAPSHOTS_DIR_NAME: &str = "snapshots";

/// Representation of the simulation state at a certain point in time.
///
/// This representation is not fully self-sufficient, and will require the
/// project file structure for proper initialization.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Snapshot {
    pub created_at: u64,

    pub worker_count: u32,

    pub clock: u64,
    pub model: Model,
    pub entities: Vec<(EntityName, Entity)>,
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
    pub fn to_bytes(&self, human_readable: bool, compress: bool) -> Result<Vec<u8>> {
        let mut data: Vec<u8> = if human_readable {
            serde_json::to_vec(&self)?
        } else {
            bincode::serialize(&self).map_err(|e| Error::FailedCreatingSnapshot(e.to_string()))?
        };
        #[cfg(feature = "lz4")]
        {
            if compress && !human_readable {
                data = lz4::block::compress(&data, None, true)?;
            }
        }
        Ok(data)
    }
}
