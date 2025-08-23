use crate::query::QueryProduct;
use crate::var::Var;
use crate::Address;
use fnv::FnvHashMap;
use std::str::FromStr;

use super::service::bigworlds;

impl From<QueryProduct> for bigworlds::QueryProduct {
    fn from(product: QueryProduct) -> Self {
        match product {
            QueryProduct::Var(vars) => {
                let var_list = bigworlds::VarList {
                    vars: vars.into_iter().map(|v| v.into()).collect(),
                };
                Self {
                    data: Some(bigworlds::query_product::Data::VarList(var_list)),
                }
            }
            QueryProduct::AddressedVar(map) => {
                let entries: Vec<bigworlds::AddressedVarEntry> = map
                    .into_iter()
                    .map(|(addr, var)| bigworlds::AddressedVarEntry {
                        address: addr.to_string(),
                        value: Some(var.into()),
                    })
                    .collect();
                let addressed_var_map = bigworlds::AddressedVarMap { entries };
                Self {
                    data: Some(bigworlds::query_product::Data::AddressedVarMap(
                        addressed_var_map,
                    )),
                }
            }
            QueryProduct::Empty => {
                let empty = bigworlds::Empty {};
                Self {
                    data: Some(bigworlds::query_product::Data::Empty(empty)),
                }
            }
            _ => {
                // Fallback to empty for unimplemented variants
                let empty = bigworlds::Empty {};
                Self {
                    data: Some(bigworlds::query_product::Data::Empty(empty)),
                }
            }
        }
    }
}

impl From<Var> for bigworlds::Var {
    fn from(var: Var) -> Self {
        match var {
            Var::String(s) => Self {
                value: Some(bigworlds::var::Value::StringValue(s)),
            },
            Var::Int(i) => Self {
                value: Some(bigworlds::var::Value::IntValue(i)),
            },
            Var::Float(f) => Self {
                value: Some(bigworlds::var::Value::FloatValue(f)),
            },
            Var::Bool(b) => Self {
                value: Some(bigworlds::var::Value::BoolValue(b)),
            },
            Var::Byte(b) => Self {
                value: Some(bigworlds::var::Value::ByteValue(vec![b])),
            },
            Var::Vec2(x, y) => Self {
                value: Some(bigworlds::var::Value::Vec2Value(bigworlds::Vec2 { x, y })),
            },
            Var::Vec3(x, y, z) => Self {
                value: Some(bigworlds::var::Value::Vec3Value(bigworlds::Vec3 {
                    x,
                    y,
                    z,
                })),
            },
            Var::List(list) => {
                let var_list = bigworlds::VarList {
                    vars: list.into_iter().map(|v| v.into()).collect(),
                };
                Self {
                    value: Some(bigworlds::var::Value::ListValue(var_list)),
                }
            }
            Var::Grid(grid) => {
                let rows: Vec<bigworlds::VarRow> = grid
                    .into_iter()
                    .map(|row| bigworlds::VarRow {
                        cells: row.into_iter().map(|v| v.into()).collect(),
                    })
                    .collect();
                let var_grid = bigworlds::VarGrid { rows };
                Self {
                    value: Some(bigworlds::var::Value::GridValue(var_grid)),
                }
            }
            Var::Map(map) => {
                let entries: Vec<bigworlds::VarValueMapEntry> = map
                    .into_iter()
                    .map(|(k, v)| bigworlds::VarValueMapEntry {
                        key: k,
                        value: Some(v.into()),
                    })
                    .collect();
                let var_map = bigworlds::VarValueMap { entries };
                Self {
                    value: Some(bigworlds::var::Value::MapValue(var_map)),
                }
            }
        }
    }
}

impl TryFrom<bigworlds::QueryProduct> for QueryProduct {
    type Error = crate::Error;

    fn try_from(proto: bigworlds::QueryProduct) -> Result<Self, Self::Error> {
        match proto.data {
            Some(bigworlds::query_product::Data::VarList(var_list)) => {
                let vars: Result<Vec<Var>, _> =
                    var_list.vars.into_iter().map(|v| v.try_into()).collect();
                Ok(QueryProduct::Var(vars?))
            }
            Some(bigworlds::query_product::Data::AddressedVarMap(addressed_var_map)) => {
                let mut map = FnvHashMap::default();
                for entry in addressed_var_map.entries {
                    let address = Address::from_str(&entry.address)?;
                    let var = entry.value.ok_or_else(|| {
                        crate::Error::FailedConversion(
                            "Missing value in AddressedVarEntry".to_string(),
                        )
                    })?;
                    let var = var.try_into()?;
                    map.insert(address, var);
                }
                Ok(QueryProduct::AddressedVar(map))
            }
            Some(bigworlds::query_product::Data::Empty(_)) => Ok(QueryProduct::Empty),
            None => Err(crate::Error::FailedConversion(
                "QueryProduct has no data variant".to_string(),
            )),
        }
    }
}

impl TryFrom<bigworlds::Var> for Var {
    type Error = crate::Error;

    fn try_from(proto: bigworlds::Var) -> Result<Self, Self::Error> {
        match proto.value {
            Some(bigworlds::var::Value::StringValue(s)) => Ok(Var::String(s)),
            Some(bigworlds::var::Value::IntValue(i)) => Ok(Var::Int(i)),
            Some(bigworlds::var::Value::FloatValue(f)) => Ok(Var::Float(f)),
            Some(bigworlds::var::Value::BoolValue(b)) => Ok(Var::Bool(b)),
            Some(bigworlds::var::Value::ByteValue(bytes)) => {
                if bytes.len() != 1 {
                    return Err(crate::Error::FailedConversion(
                        "Byte value must have exactly one byte".to_string(),
                    ));
                }
                Ok(Var::Byte(bytes[0]))
            }
            Some(bigworlds::var::Value::Vec2Value(vec2)) => Ok(Var::Vec2(vec2.x, vec2.y)),
            Some(bigworlds::var::Value::Vec3Value(vec3)) => Ok(Var::Vec3(vec3.x, vec3.y, vec3.z)),
            Some(bigworlds::var::Value::ListValue(var_list)) => {
                let list: Result<Vec<Var>, _> =
                    var_list.vars.into_iter().map(|v| v.try_into()).collect();
                Ok(Var::List(list?))
            }
            Some(bigworlds::var::Value::GridValue(var_grid)) => {
                let grid: Result<Vec<Vec<Var>>, _> = var_grid
                    .rows
                    .into_iter()
                    .map(|row| {
                        row.cells
                            .into_iter()
                            .map(|v| v.try_into())
                            .collect::<Result<Vec<Var>, _>>()
                    })
                    .collect();
                Ok(Var::Grid(grid?))
            }
            Some(bigworlds::var::Value::MapValue(var_map)) => {
                let map: Result<std::collections::BTreeMap<String, Var>, crate::Error> = var_map
                    .entries
                    .into_iter()
                    .map(|entry| {
                        let value = entry.value.ok_or_else(|| {
                            crate::Error::FailedConversion(
                                "Missing value in VarValueMapEntry".to_string(),
                            )
                        })?;
                        let value = value.try_into()?;
                        Ok::<(String, Var), crate::Error>((entry.key, value))
                    })
                    .collect();
                Ok(Var::Map(map?))
            }
            None => Err(crate::Error::FailedConversion(
                "Var has no value variant".to_string(),
            )),
        }
    }
}
