use fnv::FnvHashMap;

use crate::{
    query::{self, Limits},
    time::Instant,
    worker, Address, Error, Result, VarType,
};

use super::{AddressedTypedMap, Description, Filter, Layout, Map, Query, QueryProduct};

pub async fn process_query(
    query: &Query,
    part: &mut worker::part::Partition,
) -> Result<QueryProduct> {
    let now = Instant::now();

    let mut selected_entities = part.entities.keys().collect::<Vec<&String>>();
    let mut selected_entities_archived = if query.archive == query::Archive::Include {
        part.archive
            .as_ref()
            .ok_or(Error::ArchiveDisabled)?
            .keys()
            .filter_map(|k| k.ok())
            .collect::<Vec<fjall::Slice>>()
    } else {
        vec![]
    };
    // println!("stringids took: {}ms", now.elapsed().as_millis());

    // NOTE: only using part.get_entity() counts towards entity *access* as
    // understood by the system at large. Reading entities during the filtering
    // phase doesn't count towards real access.

    // First apply filters and get a list of selected entities.
    for filter in &query.filters {
        let mut to_retain = Vec::new();
        let mut to_retain_archive = Vec::new();
        let insta = Instant::now();
        match filter {
            Filter::Id(desired_ids) => {
                unimplemented!()
                // for desired_id in desired_ids {
                //     let string_id = desired_id.to_string();
                //     if !selected_entities.contains(&&string_id) {
                //         continue;
                //     }
                //     to_retain.push(&string_id);
                // }
            }
            Filter::Name(desired_names) => {
                for desired_name in desired_names {
                    if query.archive != query::Archive::Ignore {
                        let name_bytes: fjall::Slice = desired_name.into();
                        if selected_entities_archived.contains(&name_bytes) {
                            to_retain_archive.push(name_bytes);
                        }
                    }

                    if selected_entities.contains(&desired_name) {
                        to_retain.push(desired_name);
                    }
                }

                // for selected_entity in &selected_entities {
                //     if !desired_names.contains(selected_entity) {
                //         continue;
                //     }
                //     to_retain.push(selected_entity.to_owned());
                // }
            }
            Filter::AllComponents(desired_components) => {
                'ent: for entity_id in &selected_entities {
                    // if let Ok(entity) = part.get_entity(entity_id) {
                    if let Some(entity) = part.entities.get(entity_id.as_str()) {
                        for desired_component in desired_components {
                            if !entity.components.contains(desired_component) {
                                continue 'ent;
                            }
                        }
                        to_retain.push(entity_id);
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
                        if let Some(entity_limit) = query.limits.entities {
                            if to_retain.len() >= entity_limit as usize {
                                break;
                            }
                        }
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
                        VarType::String => {
                            // For strings we understand range filter as
                            // related to the string length.
                            let len = var.to_string().len();
                            if (len as i32) > low.to_int() && (len as i32) < high.to_int() {
                                to_retain.push(entity_name);
                            }
                        }
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
                        _ => unimplemented!(),
                    }
                }
            }
            Filter::AttrRange(_, _, _) => todo!(),
        }

        if let Limits {
            entities: Some(entity_limit),
            product_entries,
            size_bytes,
        } = query.limits
        {
            to_retain.truncate(entity_limit as usize);
            to_retain_archive.truncate(entity_limit as usize);
        }

        selected_entities = to_retain;
        selected_entities_archived = to_retain_archive;
    }

    trace!("query: selected entities: {}", selected_entities.len());
    trace!(
        "query: selected entities archived: {}",
        selected_entities_archived.len()
    );
    trace!("query time (filtering): {}", now.elapsed().as_millis());

    let now = Instant::now();

    let selected_entities = selected_entities.into_iter().cloned().collect::<Vec<_>>();
    let mut mapped_data = FnvHashMap::default();
    for mapping in &query.mappings {
        match mapping {
            Map::All => {
                if query.archive != query::Archive::Ignore {
                    for entity_id_archived in selected_entities_archived {
                        let entity_id =
                            unsafe { String::from_utf8_unchecked(entity_id_archived.to_vec()) };
                        if let Ok(entity) = part.get_entity_archived(&entity_id) {
                            for ((comp_name, var_name), var) in entity.storage.map {
                                mapped_data.insert((entity_id.clone(), comp_name, var_name), var);
                            }
                        }
                    }
                }
                for entity_id in &selected_entities {
                    if let Ok(entity) = part.get_entity(entity_id) {
                        for ((comp_name, var_name), var) in &entity.storage.map {
                            mapped_data.insert(
                                (
                                    entity_id.to_owned(),
                                    comp_name.to_owned(),
                                    var_name.to_owned(),
                                ),
                                var.clone(),
                            );
                        }
                    }
                }
                // we've selected everything, disregard other mappings
                break;
            }
            Map::Var(map_var_name) => {
                for entity_id in &selected_entities {
                    if let Ok(entity) = part.get_entity(entity_id) {
                        for ((comp_name, var_name), var) in &entity.storage.map {
                            if var_name == map_var_name {
                                mapped_data.insert(
                                    (
                                        entity_id.to_owned(),
                                        comp_name.to_owned(),
                                        var_name.to_owned(),
                                    ),
                                    var.clone(),
                                );
                            }
                        }
                    }
                }
            }
            Map::Components(map_components) => {
                for entity_id in &selected_entities {
                    if let Ok(entity) = part.get_entity(entity_id) {
                        for ((comp_name, var_name), var) in &entity.storage.map {
                            if map_components.contains(comp_name) {
                                mapped_data.insert(
                                    (
                                        entity_id.to_owned(),
                                        comp_name.to_owned(),
                                        var_name.to_owned(),
                                    ),
                                    var.clone(),
                                );
                            }
                        }
                    }
                }
            }
            Map::Component(map_component) => {
                for entity_id in &selected_entities {
                    if let Ok(entity) = part.get_entity(entity_id) {
                        for ((comp_name, var_name), var) in &entity.storage.map {
                            if comp_name == map_component {
                                mapped_data.insert(
                                    (
                                        entity_id.to_owned(),
                                        comp_name.to_owned(),
                                        var_name.to_owned(),
                                    ),
                                    var.clone(),
                                );
                            }
                        }
                    }
                }
            }
            Map::SelectAddrs(addrs) => {
                for entity_id in &selected_entities {
                    if let Ok(entity) = part.get_entity(entity_id) {
                        for addr in addrs {
                            if let Ok(var) = entity.storage.get_var(&addr.storage_index()) {
                                mapped_data.insert(
                                    (
                                        entity_id.to_owned(),
                                        addr.comp.to_owned(),
                                        addr.var_name.to_owned(),
                                    ),
                                    var.clone(),
                                );
                            }
                        }
                    }
                }
            }
            _ => unimplemented!("mapping: {:?}", mapping),
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
            Layout::String => {
                query_product = QueryProduct::String(
                    mapped_data
                        .into_iter()
                        .map(|(_, var)| var.to_string())
                        .collect(),
                );
            }
            layout => unimplemented!("description: None, layout: {:?}", layout),
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

    // println!("query_product: {:?}", query_product);
    trace!("query time (product): {}ms", now.elapsed().as_millis());

    Ok(query_product)
}
