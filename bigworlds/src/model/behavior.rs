use std::io::Read;

use fnv::FnvHashMap;

use crate::{CompName, Error, EventName, Result, VarName};

use super::intermediate;

/// All available rules for instantiating behavior based on simulation state.
///
/// Instancing targets makes it easy to express what logic should be applied
/// when from the level of the model, utilizing the model's general
/// composability principles.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum InstancingTarget {
    GlobalSingleton,
    PerWorker,
    PerEntityWithAllComponents(Vec<CompName>),
    PerEntityWithAnyComponent(Vec<CompName>),
    PerEntityWithAllVars(Vec<VarName>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Behavior {
    /// Unique name of the behavior model.
    pub name: String,
    /// List of event triggers for the behavior.
    ///
    /// Once spawned the behavior will get triggered only on events specified
    /// here.
    pub triggers: Vec<EventName>,
    /// Instancing targets.
    ///
    /// This is a collection of rules telling the runtime how it should
    /// instantiate the behavior based on simulation state.
    pub targets: Vec<InstancingTarget>,

    /// Verbosity setting for tracing purposes.
    ///
    /// Behaviors can print logs to the standard output, and this setting
    /// enables us to control those printouts.
    pub tracing: String,

    /// Inner representation of the behavior model.
    pub inner: BehaviorInner,
}

/// Basic byte array artifact storage.
#[derive(Clone, Serialize, Deserialize, PartialEq, Hash)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Artifact {
    pub bytes: Vec<u8>,
}

/// Custom debug implementation ensures we don't pollute log printouts.
impl std::fmt::Debug for Artifact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("artifact_redacted");
        Ok(())
    }
}

impl Into<Vec<u8>> for Artifact {
    fn into(self) -> Vec<u8> {
        self.bytes
    }
}

impl From<Vec<u8>> for Artifact {
    fn from(value: Vec<u8>) -> Self {
        Self { bytes: value }
    }
}

/// Inner representation of a behavior in the form of an enumeration of
/// all possible behavior kinds along with kind-dependent contents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum BehaviorInner {
    #[cfg(feature = "machine")]
    Machine {
        /// DSL script source.
        script: String,
        /// Complete machine logic.
        logic: crate::machine::Logic,
    },
    #[cfg(feature = "behavior_lua")]
    Lua {
        synced: bool,
        /// Lua script content.
        script: String,
    },
    #[cfg(feature = "behavior_dynlib")]
    Dynlib {
        /// NOTE: dynlib artifact must be written to disk before worker is able
        /// to load it and call into it.
        artifact: Artifact,
        entry: String,
        synced: bool,
        debug: bool,
    },
    #[cfg(feature = "behavior_wasm")]
    Wasm {
        /// WASM artifact can be loaded from memory, no disk-persistance
        /// required.
        artifact: Artifact,
        synced: bool,
        debug: bool,
    },
}

/// Implementation for turning the intermediate representation into a proper
/// behavior model.
impl TryFrom<intermediate::Behavior> for Behavior {
    type Error = Error;

    fn try_from(i: intermediate::Behavior) -> Result<Self> {
        let inner = match i.r#type.as_str() {
            #[cfg(feature = "behavior_lua")]
            "lua" => {
                if let Some(script) = &i.script {
                    BehaviorInner::Lua {
                        script: script.to_owned(),
                        synced: i.synced,
                    }
                } else {
                    println!("lua script path: {}", i.path);
                    unimplemented!()
                }
            }
            #[cfg(feature = "machine")]
            "machine" => {
                let (script, logic) = if let Some(script) = i.script {
                    let logic = crate::machine::Logic::from_script(&script, Some(i.path.clone()))?;
                    (script, logic)
                } else {
                    println!("script path: {}", i.path);
                    let script = crate::util::read_file(&i.path)?;
                    let logic = crate::machine::Logic::from_script(&script, Some(i.path.clone()))?;
                    (script, logic)
                };

                BehaviorInner::Machine { script, logic }
            }
            #[cfg(feature = "behavior_dynlib")]
            "rust" => {
                // Build the rust project locally using cargo
                // TODO: provide configuration for whether to build/rebuild
                // dynlib behaviors. The alternative is to just look at the
                // `lib` path and load that.
                // TODO: handle unsuccessful builds.
                // TODO: improve on the build output.
                info!("building dynlib from path: {}", i.path);
                let output = std::process::Command::new("cargo")
                    .env("CARGO_TERM_PROGRESS_WHEN", "always")
                    .env("CARGO_TERM_PROGRESS_WIDTH", "100")
                    .arg("build")
                    .arg("--quiet")
                    .arg("--release")
                    .arg("--manifest-path")
                    .arg(format!("{}/Cargo.toml", i.path))
                    .status()?;
                info!("build output: {:?}, artifact: {}", output, i.lib);

                let mut artifact = Vec::new();
                std::fs::File::open(i.lib)?.read_to_end(&mut artifact)?;

                BehaviorInner::Dynlib {
                    artifact: artifact.into(),
                    entry: i.entry,
                    synced: i.synced,
                    debug: i.debug,
                }
            }
            #[cfg(feature = "behavior_wasm")]
            "wasm" => {
                let mut artifact = Vec::new();
                println!("behavior: wasm: lib: {}", i.lib);
                std::fs::File::open(i.lib)?.read_to_end(&mut artifact)?;

                BehaviorInner::Wasm {
                    artifact: artifact.into(),
                    synced: i.synced,
                    debug: i.debug,
                }
            }
            _ => {
                // If the behavior type is not specified explicitly we need to
                // figure it out based on available information.

                // If the path field points to a script file, check if the file
                // extension can give us the type of behavior.
                // println!("source path: {}", i.path);

                if i.path.contains(".lua") {
                    #[cfg(feature = "behavior_lua")]
                    {
                        if let Some(script) = &i.script {
                            BehaviorInner::Lua {
                                script: script.to_owned(),
                                synced: i.synced,
                            }
                        } else {
                            // println!("lua script path: {}", i.path);
                            let script = crate::util::read_file(&i.path)?;
                            // println!("read lua script OK");
                            BehaviorInner::Lua {
                                script,
                                synced: i.synced,
                            }
                        }
                    }
                    #[cfg(not(feature = "behavior_lua"))]
                    return Err(Error::FeatureNotAvailable(format!(
                        "behavior path field specifies a `.lua` file, but lua feature is not available: {}", i.path,
                    )));
                }
                // If the script field is present, we assume the behavior to be
                // of type lua or machine, in that order.
                // TODO: looking at the script syntax we can fairly easily
                // determine behavior type here instead of running into
                // problems later.
                else if let Some(script) = &i.script {
                    let mut inner = None;
                    #[cfg(feature = "behavior_lua")]
                    {
                        inner = Some(BehaviorInner::Lua {
                            synced: i.synced,
                            script: script.to_owned(),
                        });
                    }

                    #[cfg(feature = "machine")]
                    if inner.is_none() {
                        let logic =
                            crate::machine::Logic::from_script(&script, Some(i.path.clone()))?;
                        inner = Some(BehaviorInner::Machine {
                            script: i.path.clone(),
                            logic,
                        });
                    };

                    inner.ok_or(Error::FeatureNotAvailable(format!(
                        "`machine`, path: {}",
                        i.path
                    )))?
                } else {
                    return Err(Error::Other(format!(
                        "unrecognized behavior type: {} [{}]",
                        i.r#type, i.path
                    )));
                }
            }
        };

        // TODO: targets need more work. Especially in terms of expressing more
        // complex targets.
        let mut targets = vec![];
        for (key, values) in i.targets {
            match key.as_str() {
                "global" => targets.push(InstancingTarget::GlobalSingleton),
                "worker" => {
                    if values.iter().any(|v| v.as_str() == "all") {
                        targets.push(InstancingTarget::PerWorker);
                    }
                    // TODO: additional worker instancing rules, e.g. resource
                    // or architecture based.
                }
                "components" => {
                    targets.push(InstancingTarget::PerEntityWithAllComponents(values));
                }
                _ => unimplemented!(),
            }
        }

        Ok(Self {
            name: i.name.clone(),
            triggers: i.triggers,
            targets,
            tracing: i.tracing,
            inner,
        })
    }
}
