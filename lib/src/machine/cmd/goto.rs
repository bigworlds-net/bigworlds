use crate::machine::cmd::CommandResult;
use crate::machine::Result;

/// Goto
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Goto {
    pub target_state: String,
}

impl Goto {
    pub fn new(args: Vec<String>) -> Result<Self> {
        Ok(Goto {
            target_state: args[0].to_owned(),
        })
    }

    pub fn execute(&self, current_state: &mut String) -> CommandResult {
        *current_state = self.target_state.clone();
        CommandResult::Continue
    }
}
