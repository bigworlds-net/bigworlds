use std::fs;
use std::fs::{create_dir_all, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Error, Result};
use uuid::Uuid;

pub(crate) fn format_elements_list(paths: &Vec<PathBuf>) -> String {
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

/// Stores provided token in the target location where it can be read.
pub fn store_token(token: &String) -> Result<()> {
    let dirs = match directories::ProjectDirs::from("", "", "bigworlds") {
        Some(dirs) => dirs,
        None => return Err(Error::msg("couldn't access default directory on system")),
    };

    let path = dirs.config_dir();
    create_dir_all(path).expect("failed creating dirs");

    let mut file = File::create(path.join("token")).expect("failed creating token file");
    file.write_all(token.as_bytes());

    Ok(())
}

pub fn retrieve_token() -> Result<Uuid> {
    let dirs = match directories::ProjectDirs::from("", "", "bigworlds") {
        Some(dirs) => dirs,
        None => return Err(Error::msg("couldn't access default directory on system")),
    };

    let path = dirs.config_dir();
    create_dir_all(path).expect("failed creating dirs");

    let mut file = File::open(path.join("token")).map_err(|e| Error::msg("token not found"))?;
    let mut string = String::new();
    file.read_to_string(&mut string)
        .expect("failed reading from token file");
    let token = Uuid::from_str(&string)?;
    Ok(token)
}
