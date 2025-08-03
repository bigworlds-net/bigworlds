//! Compare values.

extern crate getopts;

use std::collections::HashMap;

use self::getopts::{Matches, Options};
use serde_yaml::Value;

use super::{Command, CommandResult};

use crate::address::Address;
use crate::component::Component;
use crate::entity::{Entity, Storage};
use crate::model;
use crate::model::SimModel;
use crate::{Sim, VarType};

#[derive(PartialEq, Debug, Serialize, Deserialize, Clone)]
pub enum EqualType {
    AddrAddr,
    AddrVal,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Equal {
    pub type_: EqualType,
    pub var1: Address,
    pub var2: Option<Address>,
    pub test_value: Option<String>,
    pub false_: CommandResult,
    pub true_: CommandResult,
    pub pipe_out: Option<Address>,
}
impl Equal {
    pub fn from_str(mut args_str: &str) -> Result<Self, String> {
        // first separate the pipe_out ending, if there is a pipe
        // present
        let mut pipe_out = None;
        if args_str.contains("|") {
            let split: Vec<&str> = args_str.splitn(2, "|").collect::<Vec<&str>>();
            //            println!("{:?}", split);
            let pipe_addr = Address::from_str_with_context(split[1].trim(), None, None).unwrap();
            pipe_out = Some(pipe_addr);
            args_str = split[0].trim();
        }
        let mut options = Options::new();
        options.optopt("f", "false", "Result triggered when not equal.", "RESULT");
        options.optopt("t", "true", "Result triggered when equal.", "RESULT");
        //        println!("{}", options.short_usage("equal"));
        // regex for matching space-separated arguments, but
        // allowing spaces inside quotes
        unimplemented!();
        // let regex = Regex::new(r#"("[^"]+")|\S+"#).expect("failed creating new regex");
        // let mut split: Vec<String> = regex
        //     .captures_iter(args_str)
        //     .into_iter()
        //     .map(|s| s[0].to_string())
        //     .collect();
        // split = split
        //     .iter_mut()
        //     .map(|s| s.trim_matches('\"').to_string())
        //     .collect();
        // //        println!("{:?}", split);
        // let matches = match options.parse(split) {
        //     Ok(m) => m,
        //     Err(e) => return Err(e.to_string()),
        // };
        // //        println!("{:?}", matches.free);
        // // handle positional args
        // if matches.free.len() != 2 {
        //     return Err("exactly two positional args required".to_string());
        // }
        // let mut type_ = EqualType::AddrAddr;
        // //        let mut test_value = None;
        // //        let test_addr = match Address::from_str_scoped(
        // //            &matches.free[1], None, None, None) {
        // //            Ok(a) => Some(a),
        // //            Err(e) => {
        // //                type_ = EqualType::AddrVal;
        // //                test_value =
        // // Some(matches.free[1].clone());
        // // None            },
        // //        };
        // let var1 = Address::from_str_with_context(matches.free[0].trim(), None, None).unwrap();
        // let var2 = Address::from_str_with_context(matches.free[1].trim(), None, None);
        // //        let test_addr =
        // // Some(EntVarUID::from_str(&matches.free[1]).unwrap());
        //
        // //        let test_value = Some(matches.free[1].clone());
        // // handle optional args
        // let false_ = match matches.opt_present("false") {
        //     true => CommandResult::from_str(&matches.opt_str("false").unwrap()).unwrap(),
        //     false => CommandResult::BreakState,
        // };
        // let true_ = match matches.opt_present("true") {
        //     true => CommandResult::from_str(&matches.opt_str("true").unwrap()).unwrap(),
        //     false => CommandResult::Continue,
        // };
        //
        // Ok(Equal {
        //     type_,
        //     var1,
        //     var2,
        //     test_value: None,
        //     false_,
        //     true_,
        //     pipe_out,
        // })
    }
    pub fn from_map(map: &HashMap<String, Value>) -> Result<Self, String> {
        unimplemented!();
    }
    pub fn execute_loc(&self, storage: &mut Storage) -> CommandResult {
        let var1_euid = self.var1.get_storage_index();
        let var1_vt = self.var1.get_var_type();
        let mut eq = false;
        if self.type_ == EqualType::AddrVal {
            if var1_vt == VarType::Str {
                eq = *storage.get_str(&var1_euid).unwrap()
                    == self.test_value.as_ref().unwrap().clone();
            } else if var1_vt == VarType::Int {
                eq = *storage.get_int(&var1_euid).unwrap()
                    == self.test_value.as_ref().unwrap().parse::<i32>().unwrap();
            } else if var1_vt == VarType::Float {
                eq = *storage.get_float(&var1_euid).unwrap()
                    == self.test_value.as_ref().unwrap().parse::<f32>().unwrap();
            } else if var1_vt == VarType::Bool {
                eq = *storage.get_bool(&var1_euid).unwrap()
                    == self.test_value.as_ref().unwrap().parse::<bool>().unwrap();
            }
        } else if self.type_ == EqualType::AddrAddr {
            let var2_euid = self.var2.unwrap().get_storage_index();
            //            let var2_vt =
            // VarType::from_str(&self.var2.var_type).unwrap();
            if var1_vt == VarType::Str {
                eq = *storage.get_str(&var1_euid).unwrap() == *storage.get_str(&var2_euid).unwrap();
            } else if var1_vt == VarType::Int {
                eq = *storage
                    .get_int(&var1_euid)
                    .expect("failed getting int (addr)")
                    == *storage
                        .get_int(&var2_euid)
                        .expect("failed getting int (test_addr)");
            } else if var1_vt == VarType::Float {
                eq = *storage.get_float(&var1_euid).unwrap()
                    == *storage.get_float(&var2_euid).unwrap();
            } else if var1_vt == VarType::Bool {
                eq = *storage.get_bool(&var1_euid).unwrap()
                    == *storage.get_bool(&var2_euid).unwrap();
            }
        }
        match self.pipe_out {
            Some(v) => {
                if v.get_var_type() == VarType::Bool {
                    *storage.get_bool_mut(&v.get_storage_index()).unwrap() = eq;
                }
            }
            None => (),
        }
        match eq {
            true => self.true_.clone(),
            false => self.false_.clone(),
        }
    }
}

#[derive(PartialEq, Debug, Serialize, Deserialize, Clone)]
pub enum BiggerThanType {
    AddrAddr,
    AddrVal,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BiggerThan {
    pub type_: EqualType,
    pub var1: Address,
    pub var2: Option<Address>,
    pub test_value: Option<String>,
    pub false_: CommandResult,
    pub true_: CommandResult,
    pub out: Option<Address>,
}
impl BiggerThan {
    pub fn from_str(mut args_str: &str) -> Result<Self, String> {
        // first separate the pipe_out ending, if there is a pipe
        // present
        let mut pipe_out = None;
        if args_str.contains("|") {
            let split: Vec<&str> = args_str.splitn(2, "|").collect::<Vec<&str>>();
            //            println!("{:?}", split);
            let pipe_addr = Address::from_str_with_context(split[1].trim(), None, None).unwrap();
            pipe_out = Some(pipe_addr);
            args_str = split[0].trim();
        }
        let mut options = Options::new();
        options.optopt("f", "false", "Result triggered when not equal.", "RESULT");
        options.optopt("t", "true", "Result triggered when equal.", "RESULT");

        // regex for matching space-separated arguments, but
        // allowing spaces inside quotes
        unimplemented!();
        // let regex = Regex::new(r#"("[^"]+")|\S+"#).expect("failed creating new regex");
        // let mut split: Vec<String> = regex
        //     .captures_iter(args_str)
        //     .into_iter()
        //     .map(|s| s[0].to_string())
        //     .collect();
        // split = split
        //     .iter_mut()
        //     .map(|s| s.trim_matches('\"').to_string())
        //     .collect();
        // let matches = match options.parse(split) {
        //     Ok(m) => m,
        //     Err(e) => return Err(e.to_string()),
        // };
        // // handle positional args
        // if matches.free.len() != 2 {
        //     return Err("exactly two positional args required".to_string());
        // }
        // let mut type_ = EqualType::AddrAddr;
        // let var1 = Address::from_str_with_context(matches.free[0].trim(), None, None).unwrap();
        // let var2 = Address::from_str_with_context(matches.free[1].trim(), None, None);
        // // handle optional args
        // let false_ = match matches.opt_present("false") {
        //     true => CommandResult::from_str(&matches.opt_str("false").unwrap()).unwrap(),
        //     false => CommandResult::BreakState,
        // };
        // let true_ = match matches.opt_present("true") {
        //     true => CommandResult::from_str(&matches.opt_str("true").unwrap()).unwrap(),
        //     false => CommandResult::Continue,
        // };
        //
        // Ok(BiggerThan {
        //     type_,
        //     var1,
        //     var2,
        //     test_value: None,
        //     false_,
        //     true_,
        //     out: pipe_out,
        // })
    }
    pub fn execute_loc(&self, entity_db: &mut Storage) -> CommandResult {
        let var1_euid = self.var1.get_storage_index();
        let var1_vt = self.var1.get_var_type();
        let mut bt = false;
        if self.type_ == EqualType::AddrVal {
            if var1_vt == VarType::Str {
                bt = entity_db.get_str(&var1_euid).unwrap().len()
                    > self.test_value.as_ref().unwrap().len();
            } else if var1_vt == VarType::Int {
                bt = *entity_db.get_int(&var1_euid).unwrap()
                    > self.test_value.as_ref().unwrap().parse::<i32>().unwrap();
            } else if var1_vt == VarType::Float {
                bt = *entity_db.get_float(&var1_euid).unwrap()
                    > self.test_value.as_ref().unwrap().parse::<f32>().unwrap();
            }
        //            else if var1_vt == VarType::Bool {
        //                eq =
        // *entity_db.bool.get(&var1_euid).unwrap() >
        // self.test_value.as_ref().unwrap().parse::
        // <bool>().unwrap();            }
        } else if self.type_ == EqualType::AddrAddr {
            let var2_euid = self.var2.unwrap().get_storage_index();
            //            let var2_vt =
            // VarType::from_str(&self.var2.var_type).unwrap();
            if var1_vt == VarType::Str {
                bt = entity_db.get_str(&var1_euid).unwrap().len()
                    > entity_db.get_str(&var2_euid).unwrap().len();
            } else if var1_vt == VarType::Int {
                bt = *entity_db
                    .get_int(&var1_euid)
                    .expect("failed getting int (addr)")
                    > *entity_db
                        .get_int(&var2_euid)
                        .expect("failed getting int (test_addr)");
            } else if var1_vt == VarType::Float {
                bt = *entity_db.get_float(&var1_euid).unwrap()
                    > *entity_db.get_float(&var2_euid).unwrap();
            }
            //            else if var1_vt == VarType::Bool {
            //                eq =
            // *entity_db.bool.get(&var1_euid).unwrap() ==
            // *entity_db.bool.get(&var2_euid).unwrap();
            // }
        }
        match self.out {
            Some(v) => {
                if v.get_var_type() == VarType::Bool {
                    *entity_db.get_bool_mut(&v.get_storage_index()).unwrap() = bt;
                }
            }
            None => (),
        }
        match bt {
            true => self.true_.clone(),
            false => self.false_.clone(),
        }
    }
}
