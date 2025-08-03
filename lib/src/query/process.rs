use fnv::FnvHashMap;

use crate::{time::Instant, worker, Address, Result, StringId, VarType};

use super::{AddressedTypedMap, Description, Filter, Layout, Map, Query, QueryProduct};

pub async fn process_query(
    query: &Query,
    part: &mut worker::part::Partition,
) -> Result<QueryProduct> {
    let now = Instant::now();

    let mut selected_entities = part.entities.keys().map(|v| v).collect::<Vec<&StringId>>();
    let entities = &part.entities;

    // first apply filters and get a list of selected entities
    for filter in &query.filters {
        let mut to_retain = Vec::new();
        let insta = Instant::now();
        match filter {
            Filter::Id(desired_ids) => {
                unimplemented!();
                // for selected_entity_id in &selected_entities {
                //     if !desired_ids.contains(&selected_entity_id) {
                //         continue;
                //     }
                //     to_retain.push(*selected_entity_id);
                // }
            }
            Filter::Name(desired_names) => {
                for selected_entity in &selected_entities {
                    if !desired_names.contains(selected_entity) {
                        continue;
                    }
                    to_retain.push(selected_entity.to_owned());
                }
            }
            Filter::AllComponents(desired_components) => {
                'ent: for entity_id in &selected_entities {
                    if let Some(entity) = entities.get(entity_id.as_str()) {
                        for desired_component in desired_components {
                            if !entity.components.contains(desired_component) {
                                continue 'ent;
                            }
                        }
                        to_retain.push(*entity_id);
                    }
                }
            }
            Filter::Distance(x_addr, y_addr, z_addr, dx, dy, dz) => {
                unimplemented!();
                // // first get the target point position
                // let entity_id = match entity_names.get(&x_addr.entity) {
                //     Some(entity_id) => *entity_id,
                //     None => match x_addr.entity.parse() {
                //         Ok(p) => p,
                //         Err(e) => continue,
                //     },
                // };

                // // let insta = std::time::Instant::now();
                // let (x, y, z) = if let Some(entity) = entities.get(&entity_id) {
                //     (
                //         entity
                //             .storage
                //             .get_var(&x_addr.storage_index())
                //             .unwrap()
                //             .clone()
                //             .to_float(),
                //         entity
                //             .storage
                //             .get_var(&y_addr.storage_index())
                //             .unwrap()
                //             .clone()
                //             .to_float(),
                //         entity
                //             .storage
                //             .get_var(&z_addr.storage_index())
                //             .unwrap()
                //             .clone()
                //             .to_float(),
                //     )
                // } else {
                //     unimplemented!();
                // };
                // // println!(
                // //     "getting xyz took: {} ms",
                // //     Instant::now().duration_since(insta).as_millis()
                // // );

                // // let insta = std::time::Instant::now();
                // for entity_id in &selected_entities {
                //     if let Some(entity) = entities.get(entity_id) {
                //         if let Ok(pos_x) = entity
                //             .storage
                //             .get_var(&("transform".parse().unwrap(), "pos_x".parse().unwrap()))
                //         {
                //             if (pos_x.to_float() - x).abs() > *dx {
                //                 continue;
                //             }
                //         }
                //         if let Ok(pos_y) = entity
                //             .storage
                //             .get_var(&("transform".parse().unwrap(), "pos_y".parse().unwrap()))
                //         {
                //             if (pos_y.to_float() - y).abs() > *dy {
                //                 continue;
                //             }
                //         }
                //         if let Ok(pos_z) = entity
                //             .storage
                //             .get_var(&("transform".parse().unwrap(), "pos_z".parse().unwrap()))
                //         {
                //             if (pos_z.to_float() - z).abs() > *dz {
                //                 continue;
                //             }
                //         }
                //         to_retain.push(*entity_id);
                //     }
                // }
                // // println!(
                // //     "iterating entities took: {} ms",
                // //     Instant::now().duration_since(insta).as_millis()
                // // );
            }
            Filter::DistanceMultiPoint(multi) => {
                unimplemented!();
                // for (x_addr, y_addr, z_addr, dx, dy, dz) in multi {
                //     // first get the target point position
                //     let entity_id = match entity_names.get(&x_addr.entity) {
                //         Some(entity_id) => *entity_id,
                //         None => x_addr.entity.parse().unwrap(),
                //     };
                //     let (x, y, z) = if let Some(entity) = entities.get(&entity_id) {
                //         (
                //             entity
                //                 .storage
                //                 .get_var(&x_addr.storage_index())
                //                 .unwrap()
                //                 .clone()
                //                 .to_float(),
                //             entity
                //                 .storage
                //                 .get_var(&y_addr.storage_index())
                //                 .unwrap()
                //                 .clone()
                //                 .to_float(),
                //             entity
                //                 .storage
                //                 .get_var(&z_addr.storage_index())
                //                 .unwrap()
                //                 .clone()
                //                 .to_float(),
                //         )
                //     } else {
                //         unimplemented!();
                //     };

                //     for entity_id in &selected_entities {
                //         if let Some(entity) = entities.get(entity_id) {
                //             if let Ok(pos_x) = entity
                //                 .storage
                //                 .get_var(&("transform".parse().unwrap(), "pos_x".parse().unwrap()))
                //             {
                //                 if (pos_x.to_float() - x).abs() > *dx {
                //                     continue;
                //                 }
                //             }
                //             if let Ok(pos_y) = entity
                //                 .storage
                //                 .get_var(&("transform".parse().unwrap(), "pos_y".parse().unwrap()))
                //             {
                //                 if (pos_y.to_float() - y).abs() > *dy {
                //                     continue;
                //                 }
                //             }
                //             if let Ok(pos_z) = entity
                //                 .storage
                //                 .get_var(&("transform".parse().unwrap(), "pos_z".parse().unwrap()))
                //             {
                //                 if (pos_z.to_float() - z).abs() > *dz {
                //                     continue;
                //                 }
                //             }
                //             to_retain.push(*entity_id);
                //         }
                //     }
                // }
            }
            Filter::Node(node_id) => to_retain = selected_entities,
            Filter::Component(comp_name) => {
                for (id, entity) in &part.entities {
                    if entity.components.contains(comp_name) {
                        to_retain.push(id);
                    }
                }
            }
            Filter::SomeComponents(_) => todo!(),
            Filter::VarRange(addr, low, high) => {
                for entity_name in selected_entities {
                    let entity = if let Some(entity) = part.entities.get(entity_name) {
                        entity
                    } else {
                        continue;
                    };
                    let var = if let Ok(var) = entity.storage.get_var(&addr.as_storage_index()) {
                        var
                    } else {
                        continue;
                    };

                    match addr.var_type {
                        VarType::String => todo!(),
                        VarType::Int => {
                            let int = var.as_int()?;
                            if int > &low.to_int() && int < &high.to_int() {
                                to_retain.push(entity_name);
                            }
                        }
                        VarType::Float => {
                            let float = var.as_float()?;
                            if float > &low.to_float() && float < &high.to_float() {
                                to_retain.push(entity_name);
                            }
                        }
                        VarType::Bool => todo!(),
                        _ => unimplemented!(),
                    }
                }
            }
            Filter::AttrRange(_, _, _) => todo!(),
        }

        selected_entities = to_retain;
    }

    trace!("query: selected entities: {:?}", selected_entities);
    trace!(" query time (filtering): {}", now.elapsed().as_millis());

    let now = Instant::now();

    let mut mapped_data = FnvHashMap::default();
    for entity_id in selected_entities {
        for mapping in &query.mappings {
            match mapping {
                Map::All => {
                    if let Some(entity) = entities.get(entity_id) {
                        for ((comp_name, var_name), var) in &entity.storage.map {
                            mapped_data.insert((entity_id, comp_name, var_name), var);
                        }
                    }
                    // we've selected everything, disregard other mappings
                    break;
                }
                Map::Var(map_var_type, map_var_name) => {
                    if let Some(entity) = entities.get(entity_id) {
                        for ((comp_name, var_name), var) in &entity.storage.map {
                            if &var.get_type() == map_var_type && var_name == map_var_name {
                                mapped_data.insert((entity_id, comp_name, var_name), var);
                            }
                        }
                    }
                }
                Map::VarName(map_var_name) => {
                    if let Some(entity) = entities.get(entity_id) {
                        for ((comp_name, var_name), var) in &entity.storage.map {
                            if var_name == map_var_name {
                                mapped_data.insert((entity_id, comp_name, var_name), var);
                            }
                        }
                    }
                }
                Map::Components(map_components) => {
                    for map_component in map_components {
                        if let Some(entity) = entities.get(entity_id) {
                            for ((comp_name, var_name), var) in &entity.storage.map {
                                if comp_name == map_component {
                                    mapped_data.insert((entity_id, comp_name, var_name), var);
                                }
                            }
                        }
                    }
                }
                Map::Component(map_component) => {
                    if let Some(entity) = entities.get(entity_id) {
                        for ((comp_name, var_name), var) in &entity.storage.map {
                            if comp_name == map_component {
                                mapped_data.insert((entity_id, comp_name, var_name), var);
                            }
                        }
                    }
                }
                Map::SelectAddrs(addrs) => {
                    if let Some(entity) = entities.get(entity_id) {
                        for addr in addrs {
                            if let Ok(var) = entity.storage.get_var(&addr.storage_index()) {
                                mapped_data.insert((entity_id, &addr.comp, &addr.var_name), var);
                            }
                        }
                    }
                }
                _ => unimplemented!(),
            }
        }
    }

    // println!("mapped_data: {:?}", mapped_data);
    // println!(" query time (mapping): {}", now.elapsed().as_millis());

    let now = Instant::now();

    let mut query_product = QueryProduct::Empty;
    match query.description {
        Description::None => match query.layout {
            Layout::Var => {
                query_product = QueryProduct::Var(
                    mapped_data
                        .into_iter()
                        .map(|(_, var)| var.clone())
                        .collect(),
                );
            }
            _ => unimplemented!(),
        },
        Description::NativeDescribed => match query.layout {
            Layout::Var => {
                let mut map = FnvHashMap::default();
                for ((entity, component, var_name), var) in mapped_data {
                    map.insert(
                        (entity.clone(), component.clone(), var_name.clone()),
                        var.clone(),
                    );
                }
                query_product = QueryProduct::NativeAddressedVar(map);
            }
            _ => unimplemented!(),
        },
        Description::Entity => match query.layout {
            Layout::Var => {
                let mut data = FnvHashMap::default();
                for ((ent, _, _), var) in mapped_data {
                    data.insert(ent.to_owned(), var.to_owned());
                }
                query_product = QueryProduct::NameAddressedVar(data);
            }
            _ => unimplemented!(),
        },
        Description::EntityMany => match query.layout {
            Layout::Var => {
                let mut data = FnvHashMap::default();
                for ((ent, _, _), var) in mapped_data {
                    data.entry(ent.to_owned())
                        .and_modify(|vars: &mut Vec<crate::Var>| vars.push(var.clone()))
                        .or_insert(vec![var.clone()]);
                    // data.insert(ent.to_owned(), var.to_owned());
                }
                query_product = QueryProduct::NameAddressedVars(data);
            }
            _ => unimplemented!(),
        },
        Description::Addressed => match query.layout {
            Layout::Var => {
                let mut data = FnvHashMap::default();
                for ((ent_id, comp_name, var_name), var) in mapped_data {
                    let addr = Address {
                        // TODO make it optional to search for entity string name
                        // entity: entity_names
                        //     .iter()
                        //     .find(|(name, id)| id == &ent_id)
                        //     .map(|(name, _)| *name)
                        //     .unwrap_or(ent_id.to_string().parse().unwrap()),
                        entity: ent_id.to_string().parse().unwrap(),
                        comp: comp_name.clone(),
                        var_type: var.get_type(),
                        var_name: var_name.clone(),
                    };
                    data.insert(addr, var.clone());
                }
                query_product = QueryProduct::AddressedVar(data);
            }
            Layout::Typed => {
                let mut data = AddressedTypedMap::default();
                for ((ent_id, comp_name, var_name), var) in mapped_data {
                    let addr = Address {
                        // TODO make it optional to search for entity string name
                        // entity: entity_names
                        // .iter()
                        // .find(|(name, id)| id == &ent_id)
                        // .map(|(name, _)| *name)
                        // .unwrap_or(ent_id.to_string().parse().unwrap()),
                        entity: ent_id.to_string().parse().unwrap(),
                        comp: comp_name.clone(),
                        var_type: var.get_type(),
                        var_name: var_name.clone(),
                    };
                    if var.is_float() {
                        data.floats.insert(addr, var.to_float());
                    } else if var.is_bool() {
                        data.bools.insert(addr, var.to_bool());
                    } else if var.is_int() {
                        data.ints.insert(addr, var.to_int());
                    }
                }
                query_product = QueryProduct::AddressedTyped(data);
            }
            _ => unimplemented!(),
        },
        _ => unimplemented!(),
    }

    trace!("query_product: {:?}", query_product);
    trace!("query time (product): {}ms", now.elapsed().as_millis());

    Ok(query_product)
}
