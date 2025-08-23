use std::path::PathBuf;

use fnv::FnvHashMap;
use uuid::Uuid;

use crate::{util, CompName, EventName, Result, MODEL_MANIFEST_FILE};

/// Intermediate model representation. It's existence is predicated by the need
/// to resolve differences between the readily deserializable file structure
/// and the regular model representation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Model {
    #[serde(rename = "model")]
    pub meta: Meta,

    pub dependencies: FnvHashMap<String, Dependency>,

    pub scenarios: Vec<Scenario>,

    pub behaviors: Vec<Behavior>,

    pub components: Vec<Component>,
    // pub events: Vec<EventModel>,
    // pub scripts: Vec<String>,
    pub prefabs: Vec<Prefab>,
    // pub components: Vec<ComponentModel>,
    pub data: FnvHashMap<String, String>,
    pub data_files: FnvHashMap<String, String>,
    // pub data_imgs: Vec<DataImageEntry>,
    // pub services: Vec<ServiceModel>,
    /// Entities to be spawned at startup, by prefab name.
    pub entities: Vec<Entity>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Entity {
    pub id: String,
    pub prefab: Option<String>,
    pub data: FnvHashMap<String, serde_json::Value>,
    pub archived: bool,
}

impl Model {
    /// Creates a new intermediate model using provided file system.
    ///
    /// # Including linked files
    ///
    /// This function will walk the `include` paths and add the data from
    /// files specified there. End result is the full view of the model as read
    /// from the files.
    pub fn from_files<FS: vfs::FileSystem>(fs: &FS, root: Option<&str>) -> Result<Self> {
        // Read the manifest at `model.toml` into intermediate representation.
        let mut model: Model = util::struct_from_path(fs, MODEL_MANIFEST_FILE)?;
        debug!("Intermediate model manifest: {model:?}");

        // Starting at the manifest, follow the `include`s recursively and
        // merge data from all the files found this way.

        let mut to_include = model.meta.includes.clone();

        // Start the include loop. Once it ends we should have all model files
        // merged into the top level model.
        loop {
            // Pop another include.
            if let Some(include) = to_include.pop() {
                trace!("including from path: {include}");

                // Handle dirs and wildcard expansion.
                let path = if include.ends_with("/*") {
                    PathBuf::from(&include[..include.len() - 2])
                } else {
                    PathBuf::from(&include)
                };
                if let Ok(dir) = fs.read_dir(path.to_str().unwrap()) {
                    for entry in dir {
                        to_include.push(path.join(entry).to_string_lossy().to_string());
                    }
                    continue;
                }

                // Read the include.
                let mut include_model: Model =
                    util::struct_from_path(fs, &include).map_err(|e| {
                        crate::Error::Other(format!("unable to read include at `{}`: {e}", include))
                    })?;
                // println!("include_model at {include}: {include_model:?}");

                // Find the parent path for the include path in question.
                let include_path = PathBuf::from(include);
                let include_parent = include_path.parent().unwrap();

                // Normalize sub-includes' paths to start from parent path.
                let normalized_includes = include_model
                    .meta
                    .includes
                    .iter()
                    .map(|i| {
                        let path = PathBuf::from(i);
                        let path = include_parent.join(path);
                        path.to_str().unwrap().to_string()
                    })
                    .collect::<Vec<_>>();

                // For good measure insert back the normalized includes.
                include_model.meta.includes = normalized_includes.clone();

                // Add all found includes to the queue.
                to_include.extend(normalized_includes);

                // Normalize other paths, allowing to specify local paths as
                // relative to the file where they are defined.
                for behavior in &mut include_model.behaviors {
                    if behavior.r#type.as_str() == "rust" {
                        if behavior.debug {
                            behavior.lib = format!("target/debug/lib{}.so", behavior.name);
                        } else {
                            behavior.lib = format!("target/release/lib{}.so", behavior.name);
                        }
                    }

                    if behavior.path.is_empty() {
                        if behavior.r#type.as_str() != "rust" {
                            behavior.path = include_path.to_str().unwrap().to_string();
                        } else {
                            behavior.path =
                                format!("{}/{}", include_parent.to_string_lossy(), behavior.path);
                        }
                        behavior.lib =
                            format!("{}/{}", include_parent.to_string_lossy(), behavior.lib);
                    } else {
                        behavior.lib = format!(
                            "{}/{}/{}",
                            include_parent.to_string_lossy(),
                            behavior.path,
                            behavior.lib
                        );
                        behavior.path =
                            format!("{}/{}", include_parent.to_string_lossy(), behavior.path);
                    }
                }

                // println!("include_model.behaviors: {:?}", include_model.behaviors);

                // Merge the include into the main model.
                model.merge(include_model);
            } else {
                break;
            }
        }

        Ok(model)
    }

    pub fn merge(&mut self, other: Model) {
        self.meta.merge(other.meta);
        self.scenarios.extend(other.scenarios);
        self.behaviors.extend(other.behaviors);
        self.components.extend(other.components);
        self.prefabs.extend(other.prefabs);
        self.entities.extend(other.entities);
        self.data.extend(other.data);
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub name: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    #[serde(rename = "include")]
    pub includes: Vec<String>,
}

impl Meta {
    pub fn merge(&mut self, other: Meta) {
        if self.name.is_none() {
            self.name = other.name;
        }
        if self.description.is_none() {
            self.description = other.description;
        }
        if self.author.is_none() {
            self.author = other.author;
        }
        self.includes.extend(other.includes);
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Dependency {
    pub path: Option<String>,
    pub git: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Scenario {
    pub name: String,
    pub data: FnvHashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefab {
    pub name: String,
    pub components: Vec<String>,
}

/// Intermediate representation of the behavior is intended to neatly capture
/// all possible options in a single structure. Depending on what kind of
/// behavior is being declared, various fields may or may not be used.
///
/// For example when including a simple `lua` script from a file, we might
/// declare the behavior with just a single `path` field pointing to the file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Behavior {
    /// Name of the behavior. Defaults to a unique uuid.
    pub name: String,
    /// Type of the behavior based on what is driving the logic, e.g.
    /// `rust`, `lua`, `bw`.
    pub r#type: String,
    /// Path to source.
    pub path: String,
    /// Entry point for behavior execution. This is currently only used for
    /// dynamic library behaviors.
    pub entry: String,
    /// Path to dynamic library artifact.
    pub lib: String,
    /// Script contents provided *inline*. This enables quickly embedding
    /// dsl or lua scripts directly inside the model files.
    pub script: Option<String>,

    /// Flag for whether the behavior should use the synced or unsynced
    /// approach for interfacing with the worker.
    pub synced: bool,
    pub debug: bool,
    pub tracing: String,

    pub triggers: Vec<EventName>,
    pub targets: FnvHashMap<String, Vec<String>>,
}

impl Default for Behavior {
    fn default() -> Self {
        Self {
            name: Uuid::new_v4().simple().to_string(),
            r#type: "".to_owned(),
            path: "".to_owned(),
            entry: "".to_owned(),
            lib: "".to_owned(),
            synced: false,
            debug: false,
            tracing: "".to_owned(),
            script: None,
            triggers: vec!["step".to_owned()],
            targets: FnvHashMap::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Component {
    pub name: String,
    pub vars: FnvHashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ScenarioData {
    Number(i32),
    String(String),
    Map(FnvHashMap<String, String>),
    Csv(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataEntry {
    // Simple((String, String)),
    // List((String, Vec<String>)),
    // Grid((String, Vec<Vec<String>>)),
}
