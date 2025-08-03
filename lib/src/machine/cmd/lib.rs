extern crate libloading;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use self::libloading::{Library, Symbol};
use serde_yaml::Value;

use crate::common::{Address, EntityId, Int, VarType};
use crate::entity::{Entity, Storage};
use crate::error::{Error, Result};
use crate::machine::cmd::{Command, CommandResult};
use crate::machine::Libraries;
use crate::model::SimModel;
use crate::{model, util, SimHandle};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LibCallSign {
    Void,
    VoidEntity,
    VoidArg(VarType),
    VoidArgArg(VarType, VarType),
    Ret(VarType),
    RetArg(VarType, VarType),
    RetArgArg(VarType, VarType, VarType),
    Var(VarType),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibCall {
    lib: String,
    func_name: String,
    func_signature: LibCallSign,
    args: Vec<String>,
    pipe_out: Option<Address>,
}
impl LibCall {
    pub fn new(args: Vec<String>) -> Result<Command> {
        // first separate the pipe_out ending, if there is a pipe
        // present
        let mut pipe_out = None;
        // TODO
        // if args_str.contains("|") {
        //     let split: Vec<&str> = args_str.splitn(2, "|").collect::<Vec<&str>>();
        //     args_str = split[0].trim();
        //     let pipe_addr = Address::from_str_with_context(split[1].trim(), None, None).unwrap();
        //     pipe_out = Some(pipe_addr);
        // }

        // let split: Vec<&str> = args_str.split(" ").collect();
        let sign = args[1].trim();
        let sign_split: Vec<&str> = sign.split(".").collect::<Vec<&str>>();
        let mut signature = LibCallSign::VoidEntity;
        let mut ret = None;
        let mut vt1 = None;
        let mut vt2 = None;
        // let mut args = Vec::new();
        println!("sign_split: {:?}", sign_split);
        match sign_split[0] {
            "fn" => {
                ret = None;
            }
            "var" => {}
            _ => {
                if sign_split[0].starts_with("fn->") {
                    ret =
                        VarType::from_str(sign_split[0].split("->").collect::<Vec<&str>>()[1]).ok();
                }
            }
            //            "var" => match sign_split
            _ => (),
        }
        match sign_split.get(1) {
            Some(one) => {
                vt1 = VarType::from_str(one).ok();
                match sign_split.get(2) {
                    Some(two) => {
                        vt2 = VarType::from_str(two).ok();
                    }
                    None => (),
                }
            }
            None => (),
        }
        signature = match ret {
            None => match vt1 {
                Some(v1) => match vt2 {
                    Some(v2) => LibCallSign::VoidArgArg(v1, v2),
                    None => LibCallSign::VoidArg(v1),
                },
                None => LibCallSign::VoidEntity,
            },
            Some(r) => match vt1 {
                Some(v1) => match vt2 {
                    Some(v2) => LibCallSign::RetArgArg(r, v1, v2),
                    None => LibCallSign::RetArg(r, v1),
                },
                None => LibCallSign::Ret(r),
            },
        };

        let cmd = Command::LibCall(LibCall {
            lib: args[0].to_string(),
            func_name: args[2].to_string(),
            func_signature: signature,
            args: Default::default(),
            pipe_out,
        });
        println!("lib_call: {:?}", cmd);
        Ok(cmd)
    }
    pub fn from_map(map: &HashMap<String, Value>) -> Result<Self> {
        unimplemented!()
    }
}
impl LibCall {
    pub fn execute_loc(
        &self,
        libs: &Libraries,
        entity_id: &EntityId,
        mut storage: &mut Storage,
    ) -> CommandResult {
        debug!("executing lib_call: {:?}, libs: {:?}", self, libs);
        //        let lock = libs.try_lock().expect("failed to lock
        // arcmut");
        let lib = libs.get(&self.lib).expect("failed to get lib from lock");
        //        println!("{:?}", self.func_signature);
        unsafe {
            // try getting symbol from lib
            match self.func_signature {
                LibCallSign::Void => {
                    let func: libloading::Symbol<unsafe extern "C" fn()> =
                        match lib.get(self.func_name.as_bytes()) {
                            Ok(f) => f,
                            Err(e) => panic!("{}", e),
                        };
                    func();
                }
                LibCallSign::VoidEntity => {
                    let mut func: libloading::Symbol<
                        unsafe extern "C" fn(&EntityId, &mut Storage) -> CommandResult,
                    > = lib.get(self.func_name.as_bytes()).unwrap();
                    let result: CommandResult = func(&entity_id, &mut storage);
                    // func(&entity_id, &mut storage, &mut result);
                    debug!("called VoidEntity function, result: {:?}", result);
                }
                LibCallSign::VoidArg(arg_vt) => match arg_vt {
                    VarType::IntGrid => {
                        unimplemented!();
                        // let func: libloading::Symbol<unsafe extern "C" fn(&mut Vec<Vec<i32>>)> =
                        //     match lib.get(self.func_name.as_bytes()) {
                        //         Ok(f) => f,
                        //         Err(e) => panic!("{}", e),
                        //     };
                        // let uid = es
                        //     .get_ref("map/regions/int_grid/main")
                        //     .expect("failed getting ref")
                        //     .uid;
                        // let mut grid = es.int_grid.get_mut(&uid);
                        // if !grid.is_some() {
                        //     return CommandResult::Ok;
                        // }
                        // func(&mut grid.unwrap());
                        // println!("called func VoidArg")
                    }
                    _ => {
                        warn!("voidarg unimplemented");
                    }
                },
                LibCallSign::Ret(ret_vt) => {
                    match ret_vt {
                        VarType::Int => {
                            let func: libloading::Symbol<unsafe extern "C" fn() -> i32> =
                                match lib.get(self.func_name.as_bytes()) {
                                    Ok(f) => f,
                                    Err(e) => panic!("{}", e),
                                };
                            let int: i32 = func();
                            //                            let ref_ =
                            // comp.loc_vars.get(self.pipe_out.unwrap()).unwrap();
                            *storage
                                .get_var_mut(
                                    &self.pipe_out.as_ref().unwrap().storage_index().clone(),
                                )
                                .unwrap()
                                .as_int_mut()
                                .unwrap() = int as Int;
                        }
                        _ => (),
                    }

                    //                    println!("{}",
                    // int);
                }
                _ => (),
            };
        }
        //        unsafe {
        ////            let func: libloading::Symbol<unsafe extern
        //// fn(&mut Vec<Vec<i32>>)> =
        //// lib.get(self.func.as_bytes()).unwrap();
        //            let func: libloading::Symbol<unsafe extern
        // fn(&mut Vec<Vec<i32>>)> = lib.get(self.func_name.
        // as_bytes())                .expect("failed
        // getting func symbol");
        ////            let uid =
        //// es.get_ref("prop/area/int_grid/ig").expect("failed
        //// getting ref").uid;
        //            let uid =
        // es.get_ref("map/regions/int_grid/main").expect("failed
        // getting ref").uid;            let mut grid =
        // es.int_grid.get_mut(&uid);            if !grid.is_some()
        // {                return CommandResult::Ok;
        //            }
        //            func(&mut grid.unwrap());
        //        }
        //        unsafe {
        //            let func: libloading::Symbol<unsafe extern
        // fn()> = lib.get(self.func.as_bytes()).unwrap();
        // func();        }
        CommandResult::Continue
    }
}
