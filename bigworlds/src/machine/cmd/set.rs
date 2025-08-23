use std::str::FromStr;

use crate::address::{Address, LocalAddress, ShortLocalAddress};
use crate::{address, CompName, EntityId, EntityName, Var, VarType};

use crate::entity::{Entity, Storage};
use crate::machine::{Error, ErrorKind, Machine, Result};

use super::super::LocationInfo;
use super::{Command, CommandResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Set {
    target: Target,
    source: Source,
    out: Option<ShortLocalAddress>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Target {
    Address(Address),
    LocalAddress(ShortLocalAddress),
}

impl Target {
    pub fn from_str(s: &str, location: &LocationInfo) -> Result<Self> {
        if s.contains(address::SEPARATOR_SYMBOL) {
            let split = s.split(address::SEPARATOR_SYMBOL).collect::<Vec<&str>>();
            if split.len() == 2 {
                return Ok(Target::LocalAddress(ShortLocalAddress {
                    comp: None,
                    var_type: VarType::from_str(split[0])?,
                    var_name: split[1].to_owned(),
                }));
            } else if split.len() == 3 {
                return Ok(Target::LocalAddress(ShortLocalAddress {
                    comp: Some(split[0].to_owned()),
                    var_type: VarType::from_str(split[1])?,
                    var_name: split[2].to_owned(),
                }));
            } else if split.len() == 4 {
                return Ok(Target::Address(Address {
                    entity: split[0].to_owned(),
                    comp: split[1].to_owned(),
                    var_type: VarType::from_str(split[2])?,
                    var_name: split[3].to_owned(),
                }));
            } else {
                unimplemented!()
            }
        }
        Err(Error::new(
            location.clone(),
            ErrorKind::Other("failed parsing set.target".to_string()),
        ))
    }

    pub fn var_type(&self) -> VarType {
        match self {
            Target::LocalAddress(a) => a.var_type,
            Target::Address(a) => a.var_type,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Source {
    Address(Address),
    LocalAddress(ShortLocalAddress),
    Value(Var),
}

impl Source {
    pub fn from_str(s: &str, target_type: VarType, location: &LocationInfo) -> Result<Self> {
        if s.contains(address::SEPARATOR_SYMBOL) {
            let split = s.split(address::SEPARATOR_SYMBOL).collect::<Vec<&str>>();
            if split.len() == 2 {
                return Ok(Source::LocalAddress(ShortLocalAddress {
                    comp: None,
                    var_type: VarType::from_str(split[0])?,
                    var_name: split[1].to_owned(),
                }));
            } else if split.len() == 3 {
                return Ok(Source::LocalAddress(ShortLocalAddress {
                    comp: Some(split[0].to_owned()),
                    var_type: VarType::from_str(split[1])?,
                    var_name: split[2].to_owned(),
                }));
            } else if split.len() == 4 {
                return Ok(Source::Address(Address {
                    entity: split[0].to_owned(),
                    comp: split[1].to_owned(),
                    var_type: VarType::from_str(split[2])?,
                    var_name: split[3].to_owned(),
                }));
            } else {
                unimplemented!()
            }
        } else {
            let var = match Var::from_str(s, Some(target_type)) {
                Ok(v) => v,
                Err(e) => {
                    return Err(Error::new(
                        location.clone(),
                        ErrorKind::InvalidCommandBody(format!(
                            "can't parse from source into target type: {}",
                            e
                        )),
                    ))
                }
            };
            Ok(Source::Value(var))
        }
    }
}

impl Set {
    pub fn new(args: Vec<String>, location: &LocationInfo) -> Result<Command> {
        let target = Target::from_str(&args[0], location)?;

        let mut source_str = "";
        // is '=' present?
        if args.len() > 1 {
            if args[1] == "=" {
                source_str = &args[2];
            } else {
                source_str = &args[1];
            }
        }

        let source = Source::from_str(source_str, target.var_type(), location)?;

        let mut out = None;
        if let Some((out_sign_pos, _)) = args.iter().enumerate().find(|(_, s)| s.as_str() == "=>") {
            if let Some(out_addr) = args.get(out_sign_pos + 1) {
                out = Some(out_addr.parse()?);
            }
        }

        Ok(Command::Set(Set {
            target,
            source,
            out,
        }))
    }

    pub async fn execute(
        &self,
        machine: &mut Machine,
        // entity_db: &mut Storage,
        // ent_uid: &EntityId,
        // comp_state: &mut StringId,
        // comp_name: &CompName,
        // location: &LocationInfo,
    ) -> CommandResult {
        let var_type = self.target.var_type();
        let target_addr = match &self.target {
            Target::Address(addr) => addr.clone(),
            Target::LocalAddress(addr) => {
                if addr.comp.is_none() {
                    // If there's no entity/component context then work on
                    // local registry vars.
                }
                todo!()
            } // Target::LocalAddress(loc_addr) => Address {
              //     entity: string::new_truncate(&ent_uid.to_string()),
              //     component: loc_addr.comp.clone().unwrap_or(comp_name.clone()),
              //     var_type: loc_addr.var_type,
              //     var_name: loc_addr.var_name.clone(),
              // },
        };

        match &self.source {
            Source::LocalAddress(loc_addr) => {
                machine
                    .set_var(
                        target_addr,
                        machine
                            .get_var(
                                loc_addr
                                    .clone()
                                    .into_address("entity".to_owned(), "component".to_owned())
                                    .unwrap(),
                            )
                            .await
                            .unwrap(),
                    )
                    .await
                    .unwrap();
            }
            Source::Address(addr) => {
                machine
                    .set_var(target_addr, machine.get_var(addr.to_owned()).await.unwrap())
                    .await
                    .unwrap();
            }
            Source::Value(val) => {
                machine.set_var(target_addr, val.to_owned()).await.unwrap();
            }
        }
        CommandResult::Continue
    }
}
