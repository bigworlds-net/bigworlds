use crate::machine::cmd::CommandResult;
use crate::machine::Result;
use crate::{string, StringId};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
pub struct Jump {
    pub target: JumpTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
pub enum JumpTarget {
    Line(u16),
    Tag(StringId),
}

impl Jump {
    pub fn new(args: Vec<String>) -> Result<Self> {
        if let Ok(num) = args[0].parse::<u16>() {
            Ok(Jump {
                target: JumpTarget::Line(num),
            })
        } else {
            let tag = if args[0].starts_with('@') {
                string::new_truncate(&args[0][1..])
            } else {
                string::new_truncate(&args[0])
            };
            Ok(Jump {
                target: JumpTarget::Tag(tag),
            })
        }
    }
    pub fn execute_loc(&self) -> CommandResult {
        match &self.target {
            JumpTarget::Line(line) => CommandResult::JumpToLine(*line as usize),
            JumpTarget::Tag(tag) => CommandResult::JumpToTag(tag.clone()),
        }
    }
}
