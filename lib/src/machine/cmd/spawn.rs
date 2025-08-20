use std::str::FromStr;

use crate::address::ShortLocalAddress;
use crate::machine::cmd::CommandResult;
use crate::machine::{Error, ErrorKind, LocationInfo, Machine, Result};
use crate::{rpc, EntityId, EntityName, Executor, PrefabName, Signal};

use super::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Spawn {
    pub prefab: Option<PrefabName>,
    pub spawn_name: Option<EntityName>,
    pub out: Option<ShortLocalAddress>,
}

impl Into<Command> for Spawn {
    fn into(self) -> Command {
        Command::Spawn(self)
    }
}

impl Spawn {
    pub fn new(args: Vec<String>, location: &LocationInfo) -> Result<Self> {
        let matches = getopts::Options::new()
            .optopt("o", "out", "", "")
            .parse(&args)
            .map_err(|e| Error::new(location.clone(), ErrorKind::ParseError(e.to_string())))?;

        let out = matches
            .opt_str("out")
            .map(|s| ShortLocalAddress::from_str(&s))
            .transpose()?;

        if matches.free.len() == 0 {
            Ok(Self {
                prefab: None,
                spawn_name: None,
                out,
            })
        } else if matches.free.len() == 1 {
            Ok(Self {
                prefab: Some(args[0].to_owned()),
                spawn_name: None,
                out,
            })
        } else if matches.free.len() == 2 {
            Ok(Self {
                prefab: Some(args[0].to_owned()),
                spawn_name: Some(args[1].to_owned()),
                out,
            })
        } else {
            return Err(Error::new(
                location.clone(),
                ErrorKind::InvalidCommandBody("can't accept more than 2 arguments".to_string()),
            ));
        }
    }

    pub async fn execute(&self, machine: &mut Machine) -> CommandResult {
        match machine
            .worker
            .execute(Signal::from(rpc::worker::Request::SpawnEntity {
                name: self.spawn_name.clone().unwrap_or("todo".to_owned()),
                prefab: self.prefab.clone(),
            }))
            .await
        {
            Ok(res) => match res {
                Ok(Signal {
                    payload: rpc::worker::Response::Empty,
                    ..
                }) => CommandResult::Continue,
                Err(e) => CommandResult::Err(ErrorKind::CoreError(e.to_string()).into()),
                _ => unreachable!(),
            },
            Err(e) => CommandResult::Err(ErrorKind::CoreError(e.to_string()).into()),
        }
        // .inspect_err(|e| log::warn!("machine cmd: spawn: {e}"));
    }

    // pub fn execute_ext(&self, sim: &mut SimHandle, ent_uid: &EntityId) -> Result<()> {
    //     sim.spawn_entity_by_prefab_name(self.prefab.as_ref(), self.spawn_id.clone())?;
    //     Ok(())
    // }
    // pub fn execute_ext_distr(&self, sim: &mut SimHandle) -> Result<()> {
    //     // central.spawn_entity(
    //     //     self.prefab.clone(),
    //     //     self.spawn_id.clone(),
    //     //     Some(DistributionPolicy::Random),
    //     // )?;
    //     Ok(())
    // }
}
