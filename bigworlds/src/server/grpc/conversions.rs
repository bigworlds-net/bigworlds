use crate::{rpc::msg::Message, Error};

use super::service::bigworlds::{self, SubscriptionId};

impl Into<Message> for bigworlds::SubscribeRequest {
    fn into(self) -> Message {
        Message::SubscribeRequest(vec![], crate::Query::default())
    }
}

impl From<Message> for bigworlds::SubscribeResponse {
    fn from(msg: Message) -> Self {
        match msg {
            Message::SubscribeResponse(id) => Self {
                message: Some(bigworlds::subscribe_response::Message::SubscriptionId(
                    SubscriptionId { id: id.to_string() },
                )),
            },
            Message::QueryResponse(product) => Self {
                message: Some(bigworlds::subscribe_response::Message::Product(
                    product.into(),
                )),
            },
            _ => unimplemented!(),
        }
    }
}

impl Into<Message> for bigworlds::RegisterClientRequest {
    fn into(self) -> Message {
        // Convert proto encodings to internal encodings
        let encodings: Vec<crate::net::Encoding> = self
            .encodings
            .into_iter()
            .filter_map(|encoding| {
                match encoding {
                    1 => Some(crate::net::Encoding::Bincode), // protoworlds::Encoding::Bincode
                    2 => Some(crate::net::Encoding::MsgPack), // protoworlds::Encoding::Msgpack
                    3 => Some(crate::net::Encoding::Json),    // protoworlds::Encoding::Json
                    0 => None,                                // protoworlds::Encoding::Unspecified
                    _ => None,
                }
            })
            .collect();

        // Convert proto transports to internal transports
        let transports: Vec<crate::net::Transport> = self
            .transports
            .into_iter()
            .filter_map(|transport| {
                match transport {
                    1 => Some(crate::net::Transport::GrpcServer), // protoworlds::Transport::Grpc
                    2 => Some(crate::net::Transport::WebSocket),  // protoworlds::Transport::Ws
                    3 => Some(crate::net::Transport::Quic), // protoworlds::Transport::Tcp (fallback to Quic)
                    0 => None,                              // protoworlds::Transport::Unspecified
                    _ => None,
                }
            })
            .collect();

        Message::RegisterClientRequest(crate::rpc::msg::RegisterClientRequest {
            name: self.name,
            is_blocking: self.is_blocking,
            auth_token: self
                .auth_pair
                .map(|auth| format!("{}:{}", auth.username, auth.password)),
            encodings,
            transports,
        })
    }
}

impl From<Message> for bigworlds::RegisterClientResponse {
    fn from(msg: Message) -> Self {
        if let Message::RegisterClientResponse(response) = msg {
            Self {
                client_id: response.client_id,
                encoding: match response.encoding {
                    crate::net::Encoding::Bincode => bigworlds::Encoding::Bincode as i32,
                    crate::net::Encoding::MsgPack => bigworlds::Encoding::Msgpack as i32,
                    crate::net::Encoding::Json => bigworlds::Encoding::Json as i32,
                },
                transport: match response.transport {
                    crate::net::Transport::Quic => bigworlds::Transport::Unspecified as i32,
                    crate::net::Transport::WebSocket => bigworlds::Transport::Ws as i32,
                    crate::net::Transport::SecureWebSocket => bigworlds::Transport::Ws as i32,
                    crate::net::Transport::HttpServer => bigworlds::Transport::Unspecified as i32,
                    crate::net::Transport::GrpcServer => bigworlds::Transport::Grpc as i32,
                },
                redirect_to: response
                    .redirect_to
                    .map(|addr| addr.to_string())
                    .unwrap_or_default(),
            }
        } else {
            // Default response if message type doesn't match
            Self {
                client_id: String::new(),
                encoding: bigworlds::Encoding::Json as i32,
                transport: bigworlds::Transport::Grpc as i32,
                redirect_to: String::new(),
            }
        }
    }
}

impl Into<Message> for bigworlds::QueryRequest {
    fn into(self) -> Message {
        let query = if let Some(query) = self.query {
            crate::Query {
                description: match query.description {
                    0 => crate::query::Description::NativeDescribed,
                    1 => crate::query::Description::Addressed,
                    2 => crate::query::Description::Entity,
                    3 => crate::query::Description::EntityMany,
                    4 => crate::query::Description::Component,
                    5 => crate::query::Description::Var,
                    6 => crate::query::Description::ComponentVar,
                    7 => crate::query::Description::Ordered,
                    8 => crate::query::Description::None,
                    _ => crate::query::Description::None,
                },
                layout: match query.layout {
                    0 => crate::query::Layout::Var,
                    1 => crate::query::Layout::String,
                    2 => crate::query::Layout::Typed,
                    _ => crate::query::Layout::String,
                },
                filters: query.filters.into_iter().map(|f| f.into()).collect(),
                mappings: query.mappings.into_iter().map(|m| m.into()).collect(),
                scope: match query.scope {
                    0 => crate::query::Scope::Global,
                    1 => crate::query::Scope::LocalNode,
                    2 => crate::query::Scope::LocalWorker,
                    3 => crate::query::Scope::Edges(1),
                    4 => crate::query::Scope::Workers(1),
                    5 => crate::query::Scope::Time(std::time::Duration::from_secs(1)),
                    _ => crate::query::Scope::Global,
                },
                archive: crate::query::Archive::default(),
                limits: crate::query::Limits::default(),
            }
        } else {
            crate::Query::default()
        };
        Message::QueryRequest(query)
    }
}

impl From<Message> for bigworlds::QueryResponse {
    fn from(msg: Message) -> Self {
        match msg {
            Message::QueryResponse(product) => {
                // Convert the internal QueryProduct to protobuf QueryProduct
                let proto_product: bigworlds::QueryProduct = product.into();
                Self {
                    product: Some(proto_product),
                }
            }
            _ => Self {
                product: Some(bigworlds::QueryProduct {
                    data: Some(bigworlds::query_product::Data::Empty(bigworlds::Empty {})),
                }),
            },
        }
    }
}

// Filter conversions
impl From<bigworlds::Filter> for crate::query::Filter {
    fn from(filter: bigworlds::Filter) -> Self {
        match filter.kind {
            Some(bigworlds::filter::Kind::AllComponents(all_components)) => {
                crate::query::Filter::AllComponents(all_components.component_names)
            }
            Some(bigworlds::filter::Kind::SomeComponents(some_components)) => {
                crate::query::Filter::SomeComponents(some_components.component_names)
            }
            Some(bigworlds::filter::Kind::Component(component)) => {
                crate::query::Filter::Component(component.component_name)
            }
            Some(bigworlds::filter::Kind::Name(name)) => {
                crate::query::Filter::Name(name.entity_names)
            }
            Some(bigworlds::filter::Kind::Id(id)) => {
                let entity_ids: Vec<u32> = id
                    .entity_ids
                    .into_iter()
                    .filter_map(|s| s.parse::<u32>().ok())
                    .collect();
                crate::query::Filter::Id(entity_ids)
            }
            Some(bigworlds::filter::Kind::VarRange(var_range)) => {
                // Convert string addresses back to LocalAddress and Var
                let local_addr = var_range
                    .local_address
                    .parse::<crate::address::LocalAddress>()
                    .unwrap_or_else(|_| crate::address::LocalAddress {
                        comp: "default".to_string(),
                        var_type: crate::VarType::String,
                        var_name: var_range.local_address,
                    });
                let var_start = crate::var::Var::String(var_range.var_start);
                let var_end = crate::var::Var::String(var_range.var_end);
                crate::query::Filter::VarRange(local_addr, var_start, var_end)
            }
            Some(bigworlds::filter::Kind::AttrRange(attr_range)) => {
                let var_start = crate::var::Var::String(attr_range.var_start);
                let var_end = crate::var::Var::String(attr_range.var_end);
                crate::query::Filter::AttrRange(attr_range.attr_id, var_start, var_end)
            }
            Some(bigworlds::filter::Kind::Distance(distance)) => {
                let addr_x = distance
                    .address_x
                    .parse::<crate::Address>()
                    .unwrap_or_else(|_| crate::Address {
                        entity: "default".to_string(),
                        comp: "default".to_string(),
                        var_type: crate::VarType::String,
                        var_name: distance.address_x,
                    });
                let addr_y = distance
                    .address_y
                    .parse::<crate::Address>()
                    .unwrap_or_else(|_| crate::Address {
                        entity: "default".to_string(),
                        comp: "default".to_string(),
                        var_type: crate::VarType::String,
                        var_name: distance.address_y,
                    });
                let addr_z = distance
                    .address_z
                    .parse::<crate::Address>()
                    .unwrap_or_else(|_| crate::Address {
                        entity: "default".to_string(),
                        comp: "default".to_string(),
                        var_type: crate::VarType::String,
                        var_name: distance.address_z,
                    });
                crate::query::Filter::Distance(
                    addr_x,
                    addr_y,
                    addr_z,
                    distance.max_x as crate::Float,
                    distance.max_y as crate::Float,
                    distance.max_z as crate::Float,
                )
            }
            Some(bigworlds::filter::Kind::DistanceMultiPoint(distance_multi)) => {
                let points: Vec<(
                    crate::Address,
                    crate::Address,
                    crate::Address,
                    crate::Float,
                    crate::Float,
                    crate::Float,
                )> = distance_multi
                    .points
                    .into_iter()
                    .map(|point| {
                        let addr_x =
                            point
                                .address_x
                                .parse::<crate::Address>()
                                .unwrap_or_else(|_| crate::Address {
                                    entity: "default".to_string(),
                                    comp: "default".to_string(),
                                    var_type: crate::VarType::String,
                                    var_name: point.address_x,
                                });
                        let addr_y =
                            point
                                .address_y
                                .parse::<crate::Address>()
                                .unwrap_or_else(|_| crate::Address {
                                    entity: "default".to_string(),
                                    comp: "default".to_string(),
                                    var_type: crate::VarType::String,
                                    var_name: point.address_y,
                                });
                        let addr_z =
                            point
                                .address_z
                                .parse::<crate::Address>()
                                .unwrap_or_else(|_| crate::Address {
                                    entity: "default".to_string(),
                                    comp: "default".to_string(),
                                    var_type: crate::VarType::String,
                                    var_name: point.address_z,
                                });
                        (
                            addr_x,
                            addr_y,
                            addr_z,
                            point.max_x as crate::Float,
                            point.max_y as crate::Float,
                            point.max_z as crate::Float,
                        )
                    })
                    .collect();
                crate::query::Filter::DistanceMultiPoint(points)
            }
            Some(bigworlds::filter::Kind::Node(node)) => {
                let node_filter = match node.r#type {
                    0 => crate::query::NodeFilter::Local(if node.value == 0 {
                        None
                    } else {
                        Some(node.value as u32)
                    }),
                    1 => crate::query::NodeFilter::Remote(if node.value == 0 {
                        None
                    } else {
                        Some(node.value as u32)
                    }),
                    _ => crate::query::NodeFilter::Local(None),
                };
                crate::query::Filter::Node(node_filter)
            }
            None => {
                // Default to a component filter if no kind is specified
                crate::query::Filter::Component("default".to_string())
            }
        }
    }
}

impl From<crate::query::Filter> for bigworlds::Filter {
    fn from(filter: crate::query::Filter) -> Self {
        let kind = match filter {
            crate::query::Filter::AllComponents(component_names) => {
                Some(bigworlds::filter::Kind::AllComponents(
                    bigworlds::AllComponents { component_names },
                ))
            }
            crate::query::Filter::SomeComponents(component_names) => {
                Some(bigworlds::filter::Kind::SomeComponents(
                    bigworlds::SomeComponents { component_names },
                ))
            }
            crate::query::Filter::Component(component_name) => Some(
                bigworlds::filter::Kind::Component(bigworlds::ComponentFilter { component_name }),
            ),
            crate::query::Filter::Name(entity_names) => {
                Some(bigworlds::filter::Kind::Name(bigworlds::NameFilter {
                    entity_names,
                }))
            }
            crate::query::Filter::Id(entity_ids) => {
                let entity_id_strings: Vec<String> =
                    entity_ids.into_iter().map(|id| id.to_string()).collect();
                Some(bigworlds::filter::Kind::Id(bigworlds::IdFilter {
                    entity_ids: entity_id_strings,
                }))
            }
            crate::query::Filter::VarRange(local_addr, var_start, var_end) => Some(
                bigworlds::filter::Kind::VarRange(bigworlds::VarRangeFilter {
                    local_address: local_addr.to_string(),
                    var_start: var_start.to_string(),
                    var_end: var_end.to_string(),
                }),
            ),
            crate::query::Filter::AttrRange(attr_id, var_start, var_end) => Some(
                bigworlds::filter::Kind::AttrRange(bigworlds::AttrRangeFilter {
                    attr_id,
                    var_start: var_start.to_string(),
                    var_end: var_end.to_string(),
                }),
            ),
            crate::query::Filter::Distance(addr_x, addr_y, addr_z, max_x, max_y, max_z) => Some(
                bigworlds::filter::Kind::Distance(bigworlds::DistanceFilter {
                    address_x: addr_x.to_string(),
                    address_y: addr_y.to_string(),
                    address_z: addr_z.to_string(),
                    max_x: max_x as f64,
                    max_y: max_y as f64,
                    max_z: max_z as f64,
                }),
            ),
            crate::query::Filter::DistanceMultiPoint(points) => {
                let distance_filters: Vec<bigworlds::DistanceFilter> = points
                    .into_iter()
                    .map(|(addr_x, addr_y, addr_z, max_x, max_y, max_z)| {
                        bigworlds::DistanceFilter {
                            address_x: addr_x.to_string(),
                            address_y: addr_y.to_string(),
                            address_z: addr_z.to_string(),
                            max_x: max_x as f64,
                            max_y: max_y as f64,
                            max_z: max_z as f64,
                        }
                    })
                    .collect();
                Some(bigworlds::filter::Kind::DistanceMultiPoint(
                    bigworlds::DistanceMultiPointFilter {
                        points: distance_filters,
                    },
                ))
            }
            crate::query::Filter::Node(node_filter) => {
                let (filter_type, value) = match node_filter {
                    crate::query::NodeFilter::Local(Some(val)) => (0, val as i32),
                    crate::query::NodeFilter::Local(None) => (0, 0),
                    crate::query::NodeFilter::Remote(Some(val)) => (1, val as i32),
                    crate::query::NodeFilter::Remote(None) => (1, 0),
                };
                Some(bigworlds::filter::Kind::Node(bigworlds::NodeFilter {
                    r#type: filter_type,
                    value,
                }))
            }
        };
        Self { kind }
    }
}

// Map conversions
impl From<bigworlds::Map> for crate::query::Map {
    fn from(map: bigworlds::Map) -> Self {
        match map.kind {
            Some(bigworlds::map::Kind::All(_)) => crate::query::Map::All,
            Some(bigworlds::map::Kind::SelectAddrs(select_addrs)) => {
                let addresses: Vec<crate::Address> = select_addrs
                    .addresses
                    .into_iter()
                    .filter_map(|addr| addr.parse::<crate::Address>().ok())
                    .collect();
                crate::query::Map::SelectAddrs(addresses)
            }
            Some(bigworlds::map::Kind::Components(components)) => {
                crate::query::Map::Components(components.component_names)
            }
            Some(bigworlds::map::Kind::Component(component)) => {
                crate::query::Map::Component(component.component_name)
            }
            Some(bigworlds::map::Kind::Var(var)) => crate::query::Map::Var(var.var_name),
            Some(bigworlds::map::Kind::VarName(var_name)) => {
                crate::query::Map::Var(var_name.var_name)
            }
            Some(bigworlds::map::Kind::VarType(var_type)) => {
                // Convert string var_type back to VarType enum
                let var_type_enum = match var_type.var_type.as_str() {
                    "string" => crate::VarType::String,
                    "int" => crate::VarType::Int,
                    "float" => crate::VarType::Float,
                    "bool" => crate::VarType::Bool,
                    "byte" => crate::VarType::Byte,
                    "vec2" => crate::VarType::Vec2,
                    "vec3" => crate::VarType::Vec3,
                    "list" => crate::VarType::List,
                    "grid" => crate::VarType::Grid,
                    "map" => crate::VarType::Map,
                    _ => crate::VarType::String, // Default fallback
                };
                crate::query::Map::VarType(var_type_enum)
            }
            None => {
                // Default to All if no kind is specified
                crate::query::Map::All
            }
        }
    }
}

impl From<crate::query::Map> for bigworlds::Map {
    fn from(map: crate::query::Map) -> Self {
        let kind = match map {
            crate::query::Map::All => Some(bigworlds::map::Kind::All(bigworlds::MapAll {})),
            crate::query::Map::SelectAddrs(addresses) => {
                let address_strings: Vec<String> =
                    addresses.into_iter().map(|addr| addr.to_string()).collect();
                Some(bigworlds::map::Kind::SelectAddrs(bigworlds::SelectAddrs {
                    addresses: address_strings,
                }))
            }
            crate::query::Map::Components(component_names) => {
                Some(bigworlds::map::Kind::Components(bigworlds::ComponentsMap {
                    component_names,
                }))
            }
            crate::query::Map::Component(component_name) => {
                Some(bigworlds::map::Kind::Component(bigworlds::ComponentMap {
                    component_name,
                }))
            }
            crate::query::Map::Var(var_name) => {
                Some(bigworlds::map::Kind::Var(bigworlds::VarMap {
                    var_type: "".to_string(), // This field is not used in the internal Map::Var
                    var_name,
                }))
            }
            crate::query::Map::VarType(var_type) => {
                let var_type_string = match var_type {
                    crate::VarType::String => "string",
                    crate::VarType::Int => "int",
                    crate::VarType::Float => "float",
                    crate::VarType::Bool => "bool",
                    crate::VarType::Byte => "byte",
                    crate::VarType::Vec2 => "vec2",
                    crate::VarType::Vec3 => "vec3",
                    crate::VarType::List => "list",
                    crate::VarType::Grid => "grid",
                    crate::VarType::Map => "map",
                    _ => "string", // Default fallback
                };
                Some(bigworlds::map::Kind::VarType(bigworlds::VarTypeMap {
                    var_type: var_type_string.to_string(),
                }))
            }
        };
        Self { kind }
    }
}
