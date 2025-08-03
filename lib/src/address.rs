//! Common interface for referencing simulation data.

use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::entity::StorageIndex;
use crate::error::{Error, Result};
use crate::{string, CompName, EntityName, VarName, VarType};

pub const SEPARATOR_SYMBOL: &'static str = ".";

/// Entity-scope address that can also handle component-scope locality.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    any(feature = "small_stringid", feature = "tiny_stringid"),
    derive(Copy)
)]
pub struct ShortLocalAddress {
    pub comp: Option<CompName>,
    pub var_type: VarType,
    pub var_name: VarName,
}

impl FromStr for ShortLocalAddress {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let split = s
            .split(crate::address::SEPARATOR_SYMBOL)
            .collect::<Vec<&str>>();
        if split.len() == 2 {
            Ok(ShortLocalAddress {
                comp: None,
                var_type: VarType::from_str(split[0])?,
                var_name: string::new_truncate(split[1]),
            })
        } else if split.len() == 3 {
            Ok(ShortLocalAddress {
                comp: Some(string::new_truncate(split[0])),
                var_type: VarType::from_str(split[1])?,
                var_name: string::new_truncate(split[2]),
            })
        } else {
            Err(Error::InvalidLocalAddress(s.to_string()))
        }
    }
}

impl ShortLocalAddress {
    pub fn into_local_address(self, component: Option<CompName>) -> Result<LocalAddress> {
        match self.comp {
            Some(c) => match component {
                Some(_c) => Ok(LocalAddress {
                    comp: _c,
                    var_type: self.var_type,
                    var_name: self.var_name,
                }),
                None => Ok(LocalAddress {
                    comp: c,
                    var_type: self.var_type,
                    var_name: self.var_name,
                }),
            },
            None => match component {
                Some(_c) => Ok(LocalAddress {
                    comp: _c,
                    var_type: self.var_type,
                    var_name: self.var_name,
                }),
                None => Err(Error::Other(
                    "failed making into local address, missing comp name".to_string(),
                )),
            },
        }
    }

    pub fn into_address(self, entity: EntityName, comp: CompName) -> Result<Address> {
        Ok(Address {
            entity,
            comp,
            var_type: self.var_type,
            var_name: self.var_name,
        })
    }

    pub fn as_storage_index(&self, comp_id: Option<CompName>) -> Result<StorageIndex> {
        match comp_id {
            Some(c) => Ok((c, self.var_name.clone())),
            None => match &self.comp {
                Some(_c) => Ok((_c.clone(), self.var_name.clone())),
                None => Err(Error::Other(
                    "failed making storage index, short local address missing component name"
                        .to_string(),
                )),
            },
        }
    }

    pub fn as_storage_index_using(&self, comp_id: CompName) -> (CompName, VarName) {
        (comp_id, self.var_name.clone())
    }

    pub fn to_string(&self) -> String {
        match &self.comp {
            Some(c) => format!(
                "{}{SEPARATOR_SYMBOL}{}{SEPARATOR_SYMBOL}{}",
                c,
                self.var_type.to_str(),
                self.var_name
            ),
            None => format!(
                "{}{SEPARATOR_SYMBOL}{}",
                self.var_type.to_str(),
                self.var_name
            ),
        }
    }
}

/// Entity-scope address.
// TODO: consider storing `StorageIndex` directly instead of `comp` and
// `var_name` separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
pub struct LocalAddress {
    pub comp: CompName,
    pub var_type: VarType,
    pub var_name: VarName,
}

impl FromStr for LocalAddress {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let split = s
            .split(crate::address::SEPARATOR_SYMBOL)
            .collect::<Vec<&str>>();
        if split.len() == 3 {
            Ok(LocalAddress {
                comp: string::new_truncate(split[0]),
                var_type: VarType::from_str(split[1])?,
                var_name: string::new_truncate(split[2]),
            })
        } else {
            Err(Error::InvalidLocalAddress(s.to_string()))
        }
    }
}

impl LocalAddress {
    pub fn into_storage_index(self) -> StorageIndex {
        (self.comp, self.var_name)
    }

    pub fn as_storage_index(&self) -> StorageIndex {
        (self.comp.clone(), self.var_name.clone())
    }

    pub fn as_storage_index_using(&self, comp_id: CompName) -> StorageIndex {
        (comp_id, self.var_name.clone())
    }

    pub fn to_string(&self) -> String {
        format!(
            "{}{SEPARATOR_SYMBOL}{}{SEPARATOR_SYMBOL}{}",
            self.comp, self.var_type, self.var_name
        )
    }
}

/// Globally unique reference to simulation variable.
// TODO: consider storing `StorageIndex` directly instead of `comp` and
// `var_name` separately.
#[derive(Debug, Hash, Eq, PartialEq, Clone, Serialize, Deserialize)]
#[cfg_attr(
    any(feature = "small_stringid", feature = "tiny_stringid"),
    derive(Copy)
)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Address {
    pub entity: EntityName,
    pub comp: CompName,
    pub var_type: VarType,
    pub var_name: VarName,
}

impl Display for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        write!(
            f,
            "{}{}{}{}{}{}{}",
            self.entity,
            SEPARATOR_SYMBOL,
            self.comp,
            SEPARATOR_SYMBOL,
            self.var_type,
            SEPARATOR_SYMBOL,
            self.var_name
        )?;
        Ok(())
    }
}

impl FromStr for Address {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let split = s.split(SEPARATOR_SYMBOL).collect::<Vec<&str>>();
        if split.len() != 4 {
            return Err(Error::FailedCreatingAddress(s.to_string()));
        }
        Ok(Address {
            entity: string::new_truncate(split[0]),
            comp: string::new_truncate(split[1]),
            var_type: VarType::from_str(split[2])?,
            var_name: string::new_truncate(split[3]),
        })
    }
}

impl Address {
    pub fn storage_index(&self) -> (CompName, VarName) {
        (self.comp.clone(), self.var_name.clone())
    }

    pub fn to_local(self) -> LocalAddress {
        LocalAddress {
            comp: self.comp,
            var_type: self.var_type,
            var_name: self.var_name,
        }
    }

    pub fn from_local(local: LocalAddress, entity: EntityName) -> Self {
        Self {
            entity,
            comp: local.comp,
            var_type: local.var_type,
            var_name: local.var_name,
        }
    }
}

/// Potentially partial reference to simulation data point.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct PartialAddress {
    pub entity: Option<EntityName>,
    pub component: Option<CompName>,
    pub var_name: VarName,
}

impl FromStr for PartialAddress {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let split = s.split(SEPARATOR_SYMBOL).collect::<Vec<&str>>();
        if split.len() == 1 {
            Ok(PartialAddress {
                entity: None,
                component: None,
                var_name: string::new_truncate(split[0]),
            })
        } else if split.len() == 2 {
            Ok(Self {
                entity: None,
                component: Some(string::new_truncate(split[0])),
                var_name: string::new_truncate(split[1]),
            })
        } else if split.len() == 3 {
            Ok(Self {
                entity: Some(string::new_truncate(split[0])),
                component: Some(string::new_truncate(split[1])),
                var_name: string::new_truncate(split[2]),
            })
        } else {
            Err(Error::FailedCreatingAddress(s.to_string()))
        }
    }
}

impl PartialAddress {
    pub fn storage_index_using(&self, comp_id: CompName) -> (CompName, VarName) {
        (comp_id, self.var_name.clone())
    }
}
