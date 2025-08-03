use crate::{string, CompName, ShortString, StringId};

use crate::entity::{Entity, Storage};
use crate::model::{self, Model};

use crate::machine::cmd::{CommandPrototype, CommandResult, LocationInfo};
use crate::machine::error::{Error, ErrorKind};
use crate::machine::{
    CallInfo, CallStackVec, IfElseCallInfo, IfElseMetaData, Logic, Machine, ProcedureCallInfo,
    Registry,
};

/// Call a procedure by name.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Call {
    /// Name of the procedure to call.
    pub proc_name: ShortString,
}

impl Call {
    pub fn new(
        args: Vec<String>,
        location: &LocationInfo,
        commands: &Vec<CommandPrototype>,
    ) -> Result<Call, Error> {
        Ok(Call {
            proc_name: args[0].parse().unwrap(),
        })
    }

    pub async fn execute(
        &self,
        machine: &mut Machine,
        logic: &Logic,
        call_stack: &mut CallStackVec,
        location: &LocationInfo,
    ) -> CommandResult {
        // Get start and end lines for the procedure.
        let (start, end) = logic.procedures.get(&self.proc_name).unwrap();

        let line = if let Some(line) = location.line {
            line
        } else {
            return CommandResult::Err(Error::new(
                location.clone(),
                ErrorKind::MissingLocationLine,
            ));
        };

        // Push the call to the stack.
        call_stack.push(CallInfo::Procedure(ProcedureCallInfo {
            call_line: line,
            start_line: *start,
            end_line: *end,
        }));

        // Continue execution at the beginning of the called procedure.
        CommandResult::JumpToLine(start + 1)
    }
}
