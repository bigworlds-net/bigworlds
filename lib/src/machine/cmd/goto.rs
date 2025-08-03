use crate::machine::cmd::CommandResult;
use crate::machine::Result;
use crate::{string, StringId};

/// Goto
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
pub struct Goto {
    pub target_state: StringId,
}

impl Goto {
    pub fn new(args: Vec<String>) -> Result<Self> {
        Ok(Goto {
            target_state: string::new_truncate(&args[0]),
        })
    }

    pub fn execute(&self, current_state: &mut StringId) -> CommandResult {
        *current_state = self.target_state.clone();
        CommandResult::Continue
    }
}
