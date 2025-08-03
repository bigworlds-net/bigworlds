use std::{fs, io};

// TODO switch to use toml instead of yaml
pub const CONFIG_FILE: &str = "interactive.yaml";

// Adding new cfg var:
// 1. add here
// 2. add to Config struct
// 3. add to Config impl fn get and set
// 4. add to cfg-list command
pub static CFG_VARS: &[&str] = &["turn_ticks", "show_on", "show_list"];

/// Serializable configuration for the interactive interface.
#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub turn_steps: i32,
    #[serde(default)]
    pub show_on: bool,
    #[serde(default)]
    pub show_list: Vec<String>,
    #[serde(default)]
    pub prompt_format: String,
    #[serde(default)]
    pub prompt_vars: Vec<String>,
}

impl Config {
    pub fn new() -> Config {
        Config {
            turn_steps: 1,
            show_on: true,
            show_list: Vec::new(),
            prompt_format: "".to_string(),
            prompt_vars: Vec::new(),
        }
    }

    pub fn new_from_file(path: &str) -> Result<Config, io::Error> {
        let file_str = match fs::read_to_string(path) {
            Ok(f) => f,
            Err(e) => return Err(e),
        };
        match toml::from_str(&file_str) {
            Ok(c) => Ok(c),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e)),
        }
    }

    pub fn save_to_file(&self, path: &str) -> Result<(), io::Error> {
        let file_str = toml::to_string(self).unwrap();
        fs::write(path, file_str)
    }

    pub fn get(&self, name: &str) -> Result<String, io::Error> {
        match name {
            "turn_steps" => Ok(format!("{}", self.turn_steps)),
            "show_on" => Ok(format!("{}", self.show_on)),
            "show_list" => Ok(format!("{:?}", self.show_list)),
            "prompt_format" => Ok(self.prompt_format.clone()),
            "prompt_vars" => Ok(format!("{:?}", self.prompt_vars)),
            _ => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Cfg variable doesn't exist",
            )),
        }
    }

    // Set cfg var by name
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), io::Error> {
        match name {
            // TODO handle unwrap
            "turn_ticks" => {
                self.turn_steps = match value.parse::<i32>() {
                    Ok(i) => i,
                    Err(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::NotFound,
                            "Failed parsing value",
                        ))
                    }
                }
            }
            "show_on" => {
                self.show_on = match value.parse::<bool>() {
                    Ok(b) => b,
                    Err(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::NotFound,
                            "Failed parsing value",
                        ))
                    }
                }
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "Cfg variable doesn't exist",
                ))
            }
        };
        Ok(())
    }

    // Add one address to the "show" list
    pub fn show_add(&mut self, addr: &str) -> Result<(), io::Error> {
        // TODO check if address is legit
        //
        self.show_list.push(addr.to_string());
        Ok(())
    }
    // Remove one address from the "show" list at index
    pub fn show_remove(&mut self, index_str: &str) -> Result<(), &str> {
        let index = match index_str.parse::<usize>() {
            Err(e) => return Err("Failed parsing string argument to an integer index"),
            Ok(i) => i,
        };
        self.show_list.remove(index);
        Ok(())
    }
}
