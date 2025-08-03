use std::str::FromStr;

use crate::{Address, CompName, Int, StringId, Var};

use crate::entity::Storage;

use super::super::error::{Error, ErrorKind, Result};
use super::super::LocationInfo;
use super::CommandResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Range {
    pub start: String,
    pub end: String,
    pub output: Address,
}

impl Range {
    pub fn get_type() -> String {
        return "range".to_string();
    }
    pub fn new(args: Vec<String>) -> Result<Self> {
        Ok(Range {
            start: args[0].to_string(),
            end: args[1].to_string(),
            output: Address::from_str(&args[2])?,
        })
    }
}
impl Range {
    pub fn execute_loc(
        &self,
        storage: &mut Storage,
        comp_name: &CompName,
        location: &LocationInfo,
    ) -> CommandResult {
        let mut list = Vec::new();
        let start_int = self.start.parse::<Int>().unwrap();
        let end_int = self.end.parse::<Int>().unwrap();
        // println!("{}, {}", start_int, end_int);
        let mut pointer = start_int;
        for _ in 0..(end_int - start_int) {
            list.push(pointer);
            pointer = pointer + 1;
        }
        // println!("{:?}", list);
        if !storage
            .get_var(&(comp_name.clone(), self.output.var_name.clone()))
            .is_ok()
        {
            storage.insert(
                (comp_name.clone(), self.output.var_name.clone()),
                Var::List(Vec::new()),
            );
        }
        // match storage.get_var_mut(&(*comp_name, self.output.var_id)) {
        //     Ok(il) => *il = Var::List(list),
        //     Err(_) => return CommandResult::Err(Error::new(*location,
        // ErrorKind::Panic)), }
        //list;
        // println!("done");
        CommandResult::Continue
    }
}
