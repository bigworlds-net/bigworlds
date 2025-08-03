use crate::machine::cmd::{Command, CommandPrototype, CommandResult, LocationInfo};
use crate::machine::error::{Error, ErrorKind};

/// Call a procedure by name.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Query {}

impl Query {
    pub fn new(
        args: Vec<String>,
        location: &LocationInfo,
        commands: &Vec<CommandPrototype>,
    ) -> Result<Command, Error> {
        Ok(Command::Query(Query {}))
    }
}
