use crate::entity::Entity;
use crate::machine::cmd::CommandResult;
use crate::machine::Result;
use crate::{Address, SimModel};
use std::str::FromStr;

/// Detach
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
pub struct Detach {
    pub signature: Address,
    /* pub comp_model_type: SmallString,
     * pub comp_model_id: Option<SmallString>,
     * pub comp_id: SmallString, */
}
impl Detach {
    // TODO develop further
    fn from_str(args_str: &str) -> Result<Self> {
        let split: Vec<&str> = args_str.split(" ").collect();
        let signature = Address::from_str(split[0]).unwrap();
        Ok(Detach { signature })
        // if split.len() == 2 {
        // Ok(Detach {
        // comp_model_type:
        // SmallString::from_str_truncate(split[0]),
        // comp_id: ArrSSmallStringtr10::from_str_truncate(split[1]),
        // comp_model_id: None,
        //})
        //} else if split.len() == 3 {
        // Ok(Detach {
        // comp_model_type:
        // SmallString::from_str_truncate(split[0]),
        // comp_id: SmallString::from_str_truncate(split[1]),
        // comp_model_id:
        // Some(SmallString::from_str_truncate(split[2])),
        //})
        //} else {
        // Err(format!("wrong number of args"))
        //}
    }
    pub fn execute_loc(&self, ent: &mut Entity, sim_model: &SimModel) -> CommandResult {
        unimplemented!();
        // let comp_model = sim_model
        //     .get_component(
        //         &ent.model_type,
        //         &self.signature.get_comp_type_safe().unwrap(),
        //         &self.signature.get_comp_id_safe().unwrap(),
        //     )
        //     .unwrap();
        // ent.components.detach(
        //     &mut ent.storage,
        //     &comp_model,
        //     //&self.signature.comp_type.unwrap(),
        //     &self.signature.get_comp_id_safe().unwrap(),
        // );
        // CommandResult::Continue
    }
}
