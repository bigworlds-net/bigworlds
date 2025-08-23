//! Model content definitions, logic for turning deserialized data into
//! model objects.

pub mod behavior;

mod intermediate;

use std::collections::HashMap;
use std::fs::{read, read_dir, File};
use std::hash::Hash;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

use behavior::Behavior;
use fnv::FnvHashMap;
use semver::{Version, VersionReq};
use serde_with::serde_as;
use toml::Value;

use crate::address::{Address, LocalAddress, ShortLocalAddress, SEPARATOR_SYMBOL};
use crate::{var, CompName, EntityName, EventName, PrefabName, VarName, VarType};

use crate::error::{Error, Result};
use crate::{util, MODEL_MANIFEST_FILE};

#[cfg(feature = "machine")]
use crate::machine::script::{parser, preprocessor, InstructionType};
#[cfg(feature = "machine")]
use crate::machine::START_STATE_NAME;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Scenario {
    pub name: String,
}

#[serde_as]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Entity {
    pub name: EntityName,
    pub prefab: Option<PrefabName>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub data: FnvHashMap<LocalAddress, crate::Var>,
}

/// Collection of all the information defining an actionable simulation.
///
/// It's built up by reading relevant model files and "flattening" the data
/// contained within them.
///
/// # Portability
///
/// `Model` is made to be fully portable, embedding all the information as
/// found in the model files. This also includes binary artifacts for the
/// behavior dynamic libraries, which get will get built when reading the model
/// from files.
///
/// # Runtime mutability
///
/// The engine allows for large amount of runtime flexibility, including
/// introduction of changes to the globally distributed model as the simulation
/// is being run.
///
/// There are a few caveats.
///
/// Models that heavily depend on highly specialized behaviors and/or services
/// where certain model characteristics are expected, may enjoy less
/// flexibility in that regard. On the other hand those behaviors and services
/// are also allowed to introduce changes to the global model, so it's possible
/// to account for this in some respects.
///
/// Changes to the global model mean the need to perform global synchronization
/// of the model accross all the nodes. This can prove to be a relatively slow
/// operation, especially cumbersome in situations where it's necessary to
/// maintain high frequency updates.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Model {
    pub scenarios: Vec<Scenario>,
    pub events: Vec<EventModel>,
    // TODO: store as a map.
    pub prefabs: Vec<PrefabModel>,
    pub components: Vec<Component>,
    pub behaviors: Vec<Behavior>,
    pub data: Vec<DataEntry>,
    pub data_files: Vec<DataFileEntry>,
    // pub data_imgs: Vec<DataImageEntry>,
    pub services: Vec<ServiceModel>,
    /// Entities to be spawned at startup, always a name and prefab pair.
    pub entities: Vec<Entity>,
}

/// Defines translation from intermediate to regular model definition.
impl Model {
    fn from_intermediate<FS: vfs::FileSystem>(im: intermediate::Model, fs: &FS) -> Result<Self> {
        let mut model = Self::default();

        for im_scenario in im.scenarios {
            model.scenarios.push(Scenario {
                name: im_scenario.name,
            })
        }

        for im_component in im.components {
            let mut component = Component::default();
            component.name = im_component.name;
            for (mut name, value) in im_component.vars {
                if !name.contains(SEPARATOR_SYMBOL) {
                    if value.is_string() {
                        name = format!("{}{}{}", VarType::Int, SEPARATOR_SYMBOL, name);
                    } else if value.is_number() {
                        name = format!("{}{}{}", VarType::String, SEPARATOR_SYMBOL, name);
                    }
                };
                component.vars.push(Var::new(&name, value)?);
            }
            model.components.push(component);
        }

        for im_prefab in im.prefabs {
            model.prefabs.push(PrefabModel {
                name: im_prefab.name,
                components: im_prefab.components,
            });
        }

        for im_behavior in im.behaviors {
            let behavior = Behavior::from_intermediate(im_behavior, fs)?;
            model.behaviors.push(behavior);
        }

        // println!(
        //     "intermediate entities to spawn at startup: {:?}",
        //     im.entities
        // );
        for im_entity in im.entities {
            model.entities.push(Entity {
                name: im_entity.id,
                prefab: im_entity.prefab,
                data: im_entity
                    .data
                    .into_iter()
                    .map(|(addr, val)| {
                        (
                            LocalAddress::from_str(&addr).unwrap(),
                            crate::Var::from_serde(None, val),
                        )
                    })
                    .collect(),
            });
        }

        // println!("{:#?}", model);

        Ok(model)
    }
}

impl Model {
    /// Merges another `Model` into `self`.
    pub fn merge(&mut self, other: Self) -> Result<()> {
        self.events.extend(other.events);
        // self.scripts.extend(other.scripts);
        for prefab in other.prefabs {
            if self
                .prefabs
                .iter()
                .find(|p| p.name == prefab.name)
                .is_none()
            {
                self.prefabs.push(prefab);
            }
        }
        self.components.extend(other.components);
        self.data.extend(other.data);
        self.data_files.extend(other.data_files);
        // self.data_imgs.extend(other.data_imgs);
        self.services.extend(other.services);
        self.entities.extend(other.entities);
        Ok(())
    }

    pub fn from_files_at(path: PathBuf) -> Result<Self> {
        Model::from_files(&vfs::PhysicalFS::new(path), None)
    }

    /// Creates a new simulation model using provided file system.
    pub fn from_files<FS: vfs::FileSystem>(fs: &FS, root: Option<&str>) -> Result<Self> {
        // Create an intermediate model from files. We end up with the full
        // view of the model in the raw form as read from the model files.
        // In the subsequent steps the intermediate form gets normalized into
        // the regular model representation.
        let intermediate = intermediate::Model::from_files(fs, root)?;

        let mut model = Model::from_intermediate(intermediate, fs)?;

        Ok(model)
    }
}

impl Model {
    /// Get reference to entity prefab by its name.
    pub fn get_prefab(&self, name: &str) -> Option<&PrefabModel> {
        self.prefabs.iter().find(|entity| &entity.name == &name)
    }

    /// Get mutable reference to entity prefab using `type_` and `id` args.
    pub fn get_entity_mut(&mut self, name: &str) -> Option<&mut PrefabModel> {
        self.prefabs.iter_mut().find(|entity| &entity.name == name)
    }

    /// Get reference to component model using `type_` and `id` args.
    pub fn get_component(&self, name: &CompName) -> Result<&Component> {
        self.components
            .iter()
            .find(|comp| &comp.name == name)
            .ok_or(Error::NoComponentModel(name.clone()))
    }

    /// Get mutable reference to component model using `type_` and `id` args.
    pub fn get_component_mut(&mut self, name: &str) -> Option<&mut Component> {
        self.components.iter_mut().find(|comp| &comp.name == name)
    }
}

/// Service declared by a module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct ServiceModel {
    /// Unique name for the service
    pub name: String,
    pub type_: Option<String>,
    pub type_args: Option<String>,
    /// Path to executable relative to module root
    pub executable: Option<String>,
    /// Path to buildable project
    pub project: Option<String>,
    /// Defines the nature of the service
    pub managed: bool,
    /// Arguments string passed to the executable
    pub args: Vec<String>,
    pub output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct EventModel {
    pub id: EventName,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct PrefabModel {
    pub name: PrefabName,
    pub components: Vec<CompName>,
}

impl TryFrom<intermediate::Prefab> for PrefabModel {
    type Error = Error;

    fn try_from(i: intermediate::Prefab) -> Result<PrefabModel> {
        Ok(PrefabModel {
            name: i.name,
            components: i.components,
        })
    }
}

/// Entity spawn request model.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct EntitySpawnRequestModel {
    pub prefab: String,
    pub name: Option<String>,
}

/// Component model.
///
/// Components are primarily referenced by their name. Other than that
/// each component defines a list of variables and a list of event triggers.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Component {
    /// String identifier of the component
    pub name: CompName,
    /// List of variables that define the component's interface
    pub vars: Vec<Var>,
}

impl From<intermediate::Component> for Component {
    fn from(im: intermediate::Component) -> Self {
        Self {
            name: im.name,
            vars: im
                .vars
                .into_iter()
                .filter_map(|(key, value)| Var::new(&key, value).ok())
                .collect(),
        }
    }
}

/// Variable model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Var {
    pub name: VarName,
    pub type_: VarType,
    pub default: Option<crate::var::Var>,
}

impl Var {
    pub fn new(key: &str, value: serde_json::Value) -> Result<Var> {
        let addr = ShortLocalAddress::from_str(&key).or(ShortLocalAddress::from_str_with_type(
            &key,
            VarType::from_json_value(&value),
        ))?;

        Ok(Var {
            name: addr.var_name.clone(),
            type_: addr.var_type,
            default: Some(crate::var::Var::from_serde(Some(addr), value)),
        })
    }
}

/// Data entry model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum DataEntry {
    Simple((String, String)),
    List((String, Vec<String>)),
    Grid((String, Vec<Vec<String>>)),
}

/// Data file entry model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum DataFileEntry {
    Json(String),
    JsonList(String),
    JsonGrid(String),
    Yaml(String),
    YamlList(String),
    YamlGrid(String),
    CsvList(String),
    CsvGrid(String),
}

/// Data image entry model. Used specifically for importing grid data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum DataImageEntry {
    BmpU8(String, String),
    BmpU8U8U8(String, String),
    // BmpCombineU8U8U8U8Int(String, String),
    // TODO
    PngU8(String, String),
    PngU8U8U8(String, String),
    PngU8U8U8Concat(String, String),
    // PngCombineU8U8U8U8(String, String),
}
