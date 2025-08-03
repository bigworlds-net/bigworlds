use crate::{Address, CompName, StringId};

use crate::entity::Storage;
use crate::machine::cmd::{CentralRemoteCommand, Command, CommandResult};
use crate::machine::error::Error;
use crate::machine::LocationInfo;
use crate::SimHandle;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimControl {
    pub args: Vec<String>,
}

impl SimControl {
    pub fn get_type() -> String {
        return "sim".to_string();
    }
    pub fn new(args: Vec<String>) -> Result<Command, Error> {
        Ok(Command::Sim(SimControl { args }))
    }
}
impl SimControl {
    pub fn execute_loc(
        &self,
        storage: &mut Storage,
        comp_state: &StringId,
        comp_uid: &CompName,
        location: &LocationInfo,
    ) -> CommandResult {
        CommandResult::ExecCentralExt(CentralRemoteCommand::Sim(self.clone()))
    }

    pub fn execute_ext(&self, sim: &mut SimHandle) -> Result<(), Error> {
        match self.args[0].as_str() {
            "apply_model" => sim.apply_model().unwrap(),
            _ => (),
        }
        Ok(())
    }
}
