//! Contains a collection of useful utility functions.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::fs::{read, read_dir, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use toml::Value;
use vfs::VfsFileType;

use crate::error::Error;
use crate::net::Encoding;
use crate::Result;

/// Walks up the directory tree looking for the model root.
pub fn find_model_root(path: PathBuf, recursion_levels: usize) -> Result<PathBuf> {
    let mut _recursion_levels = recursion_levels;
    let mut _path = path.clone();
    while _recursion_levels > 0 {
        if _path.is_dir() {
            for entry in fs::read_dir(&_path)? {
                let entry = entry?;
                if !entry.path().is_dir() {
                    if entry.file_name() == crate::MODEL_MANIFEST_FILE {
                        return Ok(_path);
                    }
                }
            }
        }
        _recursion_levels -= 1;
        if let Some(parent_path) = _path.parent() {
            _path = parent_path.to_path_buf();
        }
    }
    Err(Error::ModelRootNotFound(
        path.to_str().unwrap().to_string(),
        recursion_levels,
    ))
}

// /// Tries to select a scenario manifest path using a given path.
// /// Basically it checks whether scenarios directory can be found in the
// /// given directory path. It selects a scenario only if there is only
// /// a single scenario present.
// pub fn get_scenario_paths(path: PathBuf) -> Result<Vec<PathBuf>> {
//     let dir_path = path.join(crate::SCENARIOS_DIR_NAME);
//     // println!("{:?}", dir_path);
//     if dir_path.exists() && dir_path.is_dir() {
//         let mut scenario_paths = Vec::new();
//         let read_dir = fs::read_dir(dir_path).ok();
//         if let Some(read_dir) = read_dir {
//             for entry in read_dir {
//                 let entry = entry.ok();
//                 if let Some(entry) = entry {
//                     let entry_path = entry.path();
//                     if entry_path.is_file() {
//                         if let Some(entry_ext) = entry_path.extension() {
//                             if entry_ext == "toml" {
//                                 scenario_paths.push(entry_path);
//                             }
//                         }
//                     }
//                 }
//             }
//         }
//         if scenario_paths.len() > 0 {
//             return Ok(scenario_paths);
//         }
//     }
//     Ok(Vec::new())
// }

/// Tries to select a scenario manifest path using a given path.
///
/// Basically it checks whether scenarios directory can be found in the
/// given directory path. It selects a scenario only if there is only
/// a single scenario present.
pub fn get_snapshot_paths(path: PathBuf) -> Result<Vec<PathBuf>> {
    let dir_path = path.join(crate::SNAPSHOTS_DIR_NAME);
    // println!("{:?}", dir_path);
    if dir_path.exists() && dir_path.is_dir() {
        let mut snapshot_paths = Vec::new();
        let read_dir = fs::read_dir(dir_path).ok();
        if let Some(read_dir) = read_dir {
            for entry in read_dir {
                let entry = entry.ok();
                if let Some(entry) = entry {
                    let entry_path = entry.path();
                    if entry_path.is_file() {
                        if let Some(entry_ext) = entry_path.extension() {
                            if entry_ext == ".snapshot" {
                                snapshot_paths.push(entry_path);
                            }
                        } else {
                            // warn!("snapshot without .snapshot extension");
                            snapshot_paths.push(entry_path);
                        }
                    }
                }
            }
        }
        if snapshot_paths.len() > 0 {
            return Ok(snapshot_paths);
        }
    }
    Ok(Vec::new())
}

pub fn read_text_file<FS: vfs::FileSystem>(fs: &FS, file: &str) -> Result<String> {
    debug!("{:?}", file);
    let mut fd = fs.open_file(file)?;
    let mut content = String::new();
    fd.read_to_string(&mut content)?;

    Ok(content)
}

/// Create a static deser object from given path using serde.
pub fn deser_struct_from_path<FS: vfs::FileSystem, T>(fs: &FS, file_path: &str) -> Result<T>
where
    for<'de> T: serde::Deserialize<'de>,
{
    let mut file = fs.open_file(file_path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let path = PathBuf::from_str(file_path).unwrap();
    let d: T = match path.extension().unwrap().to_str().unwrap() {
        "toml" | "tml" => toml::from_slice(&bytes)?,
        _ => unimplemented!("{:?}", path.to_str()),
    };
    Ok(d)
}

/// Get top level directories at the given path.
pub fn get_top_dirs_at<FS: vfs::FileSystem>(fs: &FS, dir: &str) -> Result<Vec<String>> {
    let mut paths: Vec<String> = Vec::new();
    if fs.metadata(dir)?.file_type == VfsFileType::Directory {
        for entry in fs.read_dir(dir)? {
            let entry_path = format!("{}/{}", dir, entry);
            if fs.metadata(&entry_path)?.file_type == VfsFileType::Directory {
                paths.push(entry_path);
            }
        }
    };

    Ok(paths)
}

/// Get paths to files with any of the given extensions in the provided
/// directory.
pub fn find_files_with_extension<FS: vfs::FileSystem>(
    fs: &FS,
    root: &str,
    extensions: Vec<&str>,
    recursive: bool,
    exclude: Option<Vec<String>>,
) -> Result<Vec<String>> {
    // println!("find_files_with_extension, root: {:?}", root);
    let mut paths: Vec<String> = Vec::new();
    if fs.metadata(root)?.file_type == vfs::VfsFileType::Directory {
        let dir_entry = fs.read_dir(&root)?;
        'entry: for entry in dir_entry {
            let entry_path = format!("{}/{}", root, entry);
            // println!("entry_path: {}", entry_path);
            if fs.metadata(&entry_path)?.file_type == VfsFileType::Directory && recursive {
                paths.extend(find_files_with_extension(
                    fs,
                    &entry_path,
                    extensions.clone(),
                    recursive,
                    exclude.clone(),
                )?);
            } else if fs.metadata(&entry_path)?.file_type == VfsFileType::File {
                let path = PathBuf::from_str(&entry_path).unwrap();
                let ext = path
                    .extension()
                    .unwrap_or(OsStr::new(""))
                    .to_str()
                    .unwrap_or("");
                for extension in &extensions {
                    if &ext == extension {
                        // TODO excludes
                        if let Some(ref excludes) = exclude {
                            for exclude in excludes {
                                if path
                                    .file_name()
                                    .iter()
                                    .any(|s| s.to_string_lossy().contains(exclude))
                                {
                                    continue 'entry;
                                }
                                // .unwrap_or(OsStr::new("")) == exclude {}
                            }
                        }

                        paths.push(entry_path.clone());
                        break;
                    }
                }
            }
        }
    };

    Ok(paths)
}

// /// Deserialize an object at the given path.
// pub fn deser_obj_from_path<T>(file_path: PathBuf) -> Result<T>
// where
//     for<'de> T: serde::Deserialize<'de>,
// {
//     let file_data = read(file_path)?;
//     let d: T = serde_yaml::from_slice(&file_data)?;
//     Ok(d)
// }
/// Reads a file at the given path to a String.
pub fn read_file(path: &str) -> Result<String> {
    // Create a path to the desired file
    let path = Path::new(path);
    let display = path.display();
    // info!("Reading file: {}", display);

    // Open the path in read-only mode, returns
    // `io::Result<File>`
    let mut file = File::open(&path)?;

    // Read the file contents into a string, returns
    // `io::Result<usize>`
    let mut s = String::new();
    file.read_to_string(&mut s)?;
    Ok(s)
}

/// Coerces serde_yaml value to string.
pub fn coerce_toml_val_to_string(val: &Value) -> String {
    match val {
        Value::String(v) => v.to_string(),
        Value::Float(v) => format!("{}", v),
        Value::Integer(v) => format!("{}", v),
        Value::Boolean(v) => format!("{}", v),
        Value::Array(v) => format!("{:?}", v),
        Value::Table(v) => format!("{:?}", v),
        _ => unimplemented!(),
    }
}

pub fn str_from_map_value(key: &str, serde_value: &HashMap<String, Value>) -> Result<String> {
    match serde_value.get(key) {
        Some(val) => match val.as_str() {
            Some(s) => Ok(s.to_owned()),
            None => Err(Error::Other(format!(
                "value at \"{}\" must be a string",
                key
            ))),
        },
        None => Err(Error::Other(format!("map doesn't contain \"{}\"", key))),
    }
}

/// Get a similar command based on string similarity.
pub fn get_similar(original_cmd: &str, cmd_list: &[&str]) -> Option<String> {
    use strsim::{jaro, normalized_damerau_levenshtein};
    //        let command_list = CMD_LIST;
    let mut highest_sim = 0f64;
    let mut best_cmd_string = cmd_list[0];
    for cmd in cmd_list {
        let mut j = normalized_damerau_levenshtein(cmd, original_cmd);
        if j > highest_sim {
            highest_sim = j;
            best_cmd_string = &cmd;
        }
    }
    if highest_sim > 0.4f64 {
        //            println!("{}", highest_sim);
        Some(best_cmd_string.to_owned())
    } else {
        None
    }
}
/// Truncates string to specified size (ignoring last bytes if they form a
/// partial `char`).
#[inline]
pub(crate) fn truncate_str(slice: &str, size: u8) -> &str {
    if slice.is_char_boundary(size.into()) {
        unsafe { slice.get_unchecked(..size.into()) }
    } else if (size as usize) < slice.len() {
        let mut index = size.saturating_sub(1) as usize;
        while !slice.is_char_boundary(index) {
            index = index.saturating_sub(1);
        }
        unsafe { slice.get_unchecked(..index) }
    } else {
        slice
    }
}

pub fn format_elements_list(paths: &Vec<PathBuf>) -> String {
    let mut list = String::new();
    for path in paths {
        list = format!(
            "{}\n   {}",
            list,
            path.file_stem().unwrap().to_string_lossy()
        );
    }
    list
}

/// Packs serializable object to bytes based on selected encoding.
pub fn encode<S: Serialize>(obj: S, encoding: Encoding) -> Result<Vec<u8>> {
    let packed: Vec<u8> = match encoding {
        Encoding::Bincode => bincode::serialize(&obj)?,
        Encoding::MsgPack => {
            #[cfg(not(feature = "msgpack_encoding"))]
            panic!(
                "trying to use msgpack encoding, but msgpack_encoding crate feature is not enabled"
            );
            #[cfg(feature = "msgpack_encoding")]
            {
                use rmp_serde::config::StructMapConfig;
                let mut buf = Vec::new();
                obj.serialize(&mut rmp_serde::Serializer::new(&mut buf))?;
                buf
            }
        }
        Encoding::Json => {
            #[cfg(not(feature = "json_encoding"))]
            panic!("trying to use json encoding, but json_encoding crate feature is not enabled");
            #[cfg(feature = "json_encoding")]
            {
                serde_json::to_vec(&obj)?
            }
        }
    };
    Ok(packed)
}

/// Unpacks object from bytes based on selected encoding.
pub fn decode<'de, P: Deserialize<'de>>(bytes: &'de [u8], encoding: Encoding) -> Result<P> {
    let unpacked = match encoding {
        Encoding::Bincode => bincode::deserialize(bytes)?,
        Encoding::MsgPack => {
            #[cfg(not(feature = "msgpack_encoding"))]
            panic!("trying to unpack using msgpack encoding, but msgpack_encoding crate feature is not enabled");
            #[cfg(feature = "msgpack_encoding")]
            {
                use rmp_serde::config::StructMapConfig;
                let mut de = rmp_serde::Deserializer::new(bytes).with_binary();
                Deserialize::deserialize(&mut de)?
            }
        }
        Encoding::Json => {
            #[cfg(not(feature = "json_encoding"))]
            panic!("trying to unpack using json encoding, but json_encoding crate feature is not enabled");
            #[cfg(feature = "json_encoding")]
            {
                serde_json::from_slice(bytes)?
            }
        }
    };
    Ok(unpacked)
}

// TODO allow for different compression modes
/// Compress bytes using lz4.
#[cfg(feature = "lz4")]
pub(crate) fn compress(bytes: &Vec<u8>) -> Result<Vec<u8>> {
    let compressed = lz4::block::compress(bytes.as_slice(), None, true)?;
    Ok(compressed)
}
