use crate::entity::Entity;
use crate::machine::cmd::CommandResult;
use crate::{SimModel, StringId};

/// Attach
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
pub struct Attach {
    pub model_type: StringId,
    pub model_id: StringId,
    pub new_id: StringId,
}
impl Attach {
    // fn from_str(args_str: &str) -> MachineResult<Self> {
    //     let split: Vec<&str> = args_str.split(" ").collect();
    //     Ok(Attach {
    //         model_type: StringIndex::from(split[0]).unwrap(),
    //         model_id: StringIndex::from(split[1]).unwrap(),
    //         new_id: StringIndex::from(split[2]).unwrap(),
    //     })
    // }
    pub fn execute_loc(&self, ent: &mut Entity, sim_model: &SimModel) -> CommandResult {
        unimplemented!();
        // ent.components.attach(
        //     sim_model,
        //     &mut ent.storage,
        //     self.model_type.as_str(),
        //     self.model_id.as_str(),
        //     self.new_id.as_str(),
        // );
        CommandResult::Continue
    }
}
