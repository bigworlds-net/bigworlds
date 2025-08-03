extern crate fasteval;
extern crate getopts;

use std::collections::{BTreeMap, HashMap};
use std::process::Command as ProcessCommand;

// use evalexpr::eval;
use fasteval::{Compiler, Evaler};

use crate::address::{Address, LocalAddress, ShortLocalAddress};
use crate::{string, CompName, Float, StringId, Var, VarType};

// use serde_yaml::Value;
// use shlex::split;
//
use self::getopts::Options;

// use crate::component::Component;
use crate::entity::{Entity, Storage};
// use crate::error::Error;
use crate::model::Model;

use super::super::{CommandPrototype, Error, LocationInfo, Registry, RegistryTarget, Result};
use super::{Command, CommandResult};
use crate::machine::{ErrorKind, Machine};
use std::str::FromStr;

/// Precompiles an evaluation and stores it
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Eval {
    pub expr: fasteval::Instruction,
    // pub slab: fasteval::Slab,
    pub args: Vec<(StringId, Address)>,
    // pub arg0: Option<(ShortString, RegistryTarget)>,
    pub out: Option<Address>,
}

impl Eval {
    pub fn new(args: Vec<String>) -> Result<Self> {
        let matches = getopts::Options::new()
            .optopt("o", "out", "", "")
            // .optopt("i", "in", "", "")
            .parse(&args)?;

        println!("eval matches {:?}", matches);

        let mut slab = fasteval::Slab::new();
        let parser = fasteval::Parser::new();

        // let mut expr = matches.free[0];
        // for input in matches.opt_strs("in") {

        //     expr.replacen("", , count)
        // }

        // expr.replace(from, to)

        let compiled = parser
            .parse(&matches.free[0], &mut slab.ps)
            .unwrap()
            .from(&slab.ps)
            .compile(&slab.ps, &mut slab.cs);

        // let mut out = None;
        let out = matches
            .opt_str("out")
            .map(|s| Address::from_str(&s))
            .transpose()?;

        let mut eval_args = Vec::new();
        for free_arg in matches.free.iter().skip(1) {
            let split = free_arg.split('=').collect::<Vec<&str>>();
            if split.len() == 2 {
                eval_args.push((
                    string::new_truncate(split[0]),
                    Address::from_str(&split[1])?,
                ));
            }
        }

        Ok(Self {
            expr: compiled,
            args: eval_args,
            out,
        })
    }

    pub async fn execute(
        &self,
        machine: &Machine,
        // comp_name: &CompName,
        registry: &mut Registry,
        location: &LocationInfo,
    ) -> CommandResult {
        let mut slab = fasteval::Slab::new();
        let mut ns = fasteval::StringToF64Namespace::new();
        // let mut map = BTreeMap::new();
        for (arg_name, arg_addr) in &self.args {
            let val = match machine.get_var(arg_addr.clone()).await {
                Ok(v) => v.to_float(),
                Err(e) => {
                    return CommandResult::Err(Error::new(
                        location.clone(),
                        ErrorKind::CoreError(e.to_string()),
                    ));
                }
            };
            // println!("position:x value: {}", xval);
            ns.insert(arg_name.to_string(), val as f64);
        }

        // let val = fasteval::ez_eval(&self.expr, &mut ns).unwrap();
        let val = self.expr.eval(&slab, &mut ns).unwrap();
        // let val = fasteval::eval_compiled!(self.expr, &self.slab, &mut ns);

        if let Some(out_addr) = &self.out {
            machine
                .set_var(
                    out_addr.clone(),
                    Var::from_str(&val.to_string(), Some(out_addr.var_type)).unwrap(),
                )
                .await
                .unwrap();
        }

        // match self.out {
        //     RegistryTarget::Str0 => registry.str0 =
        // ShortString::from_str_truncate(format!("{}", val)),     _ => (),
        // }

        // println!("eval result: {}", val);
        CommandResult::Continue
    }
}
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct EvalReg {
//     pub expr: String,
//     pub arg0: Option<(StringId, RegistryTarget)>,
//     pub out: RegistryTarget,
// }
// impl EvalReg {
//     pub fn new(
//         args: Vec<String>,
//         location: &LocationInfo,
//         commands: &Vec<CommandPrototype>,
//     ) -> Result<Command> {
//         let cmd = EvalReg {
//             expr: args[0].to_string(),
//             arg0: None,
//             out: RegistryTarget::Str0,
//         };
//         Ok(Command::EvalReg(cmd))
//     }
//     pub fn execute_loc(&self, registry: &mut Registry) -> CommandResult {
//         // let mut ns = fasteval::EmptyNamespace;
//         // let mut slab = fasteval::Slab::new();
//         // // let val = fasteval::ez_eval(&self.expr, &mut ns).unwrap();
//         // let val = precomps[0].eval(&slab, &mut ns).unwrap();
//         // match self.out {
//         //     RegistryTarget::Str0 => registry.str0 =
// ShortString::from_str_truncate(format!("{}", val)),         //     _ => (),
//         // }
//         //
//         // println!("eval result: {}", val);
//         CommandResult::Continue
//     }
// }
