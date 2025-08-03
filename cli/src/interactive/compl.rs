use std::ops::Deref;
use std::sync::{Arc, Mutex};

use bigworlds::SimHandle;
use linefeed::complete::{Completer, Completion};
use linefeed::terminal::Terminal;
use linefeed::Prompter;

use super::config::CFG_VARS;
use super::APP_COMMANDS;

pub struct MainCompleter {
    // pub driver: Arc<Mutex<SimDriver>>,
}

impl<Term: Terminal> Completer<Term> for MainCompleter {
    fn complete(
        &self,
        word: &str,
        prompter: &Prompter<Term>,
        start: usize,
        _end: usize,
    ) -> Option<Vec<Completion>> {
        let line = prompter.buffer();
        let mut words = line[..start].split_whitespace();

        match words.next() {
            // Complete command name
            None => {
                let mut compls = Vec::new();

                for &(cmd, _) in APP_COMMANDS {
                    if cmd.starts_with(word) {
                        compls.push(Completion::simple(cmd.to_owned()));
                    }
                }

                Some(compls)
            }
            // Complete cfg vars getting and setting
            Some("cfg") | Some("cfg-get") => {
                if words.count() == 0 {
                    let mut res = Vec::new();
                    for name in CFG_VARS {
                        if name.starts_with(word) {
                            res.push(Completion::simple(name.to_string().to_owned()));
                        }
                    }
                    Some(res)
                } else {
                    None
                }
            }
            // // Complete addresses for commands
            // Some("ls") | Some("show") | Some("show-grid") => {
            //     if words.count() == 0 {
            //         let res = match &self.driver.lock().unwrap().deref() {
            //             SimDriver::Local(sim) => complete_address_local(word, sim),
            //             SimDriver::Remote(client) => unimplemented!(),
            //             _ => unimplemented!(),
            //         };
            //         Some(res)
            //     } else {
            //         None
            //     }
            // }
            _ => None,
        }
    }
}

fn complete_address_local(word: &str, sim: &SimHandle) -> Vec<Completion> {
    unimplemented!();
    // let mut res = Vec::new();
    // // check which addr part we need to complete
    // if word.starts_with("/") && word.matches("/").count() == 1 {
    //     // entity
    //     let split = word[1..].split("/").collect::<Vec<&str>>();
    //     let wp_ent_type = split[0];
    //     let w = split[1];
    //     for e in sim.get_entities() {
    //         if wp_ent_type == e.model_type.as_str() {
    //             if e.model_id.starts_with(w) {
    //                 let complet = Completion {
    //                     completion: format!("/{}/{}", wp_ent_type, e.model_id.to_string()),
    //                     display: Some(e.model_id.to_string()),
    //                     suffix: linefeed::Suffix::Some('/'),
    //                 };
    //                 res.push(complet);
    //             }
    //         }
    //     }
    // } else if word.matches("/").count() == 2 {
    //     // comp
    //     let split = word[1..].split("/").collect::<Vec<&str>>();
    //     let wp_ent_type = split[0];
    //     let wp_ent_name = split[1];
    //     let wp_comp_type = split[2];
    //     let w = split[3];
    //     // We need to handle both components that really exist as component objects
    //     // and those that are only used for referencing vars.
    //     // We're only interested in getting unique comp entries.
    //     let mut out_comps: Vec<String> = Vec::new();
    //     for (comp_uid, comp) in &sim.get_entity_str(wp_ent_name).unwrap().components.map {
    //         //TODO
    //         // let (comp_type, comp_name) = comp_uid;
    //         // if comp_name.starts_with(w)
    //         //     && wp_ent_type
    //         //         == &sim.model
    //         //             .get_component()
    //         //             .get(comp.model_uid as usize)
    //         //             .unwrap()
    //         //             .entity_type
    //         //     && wp_comp_type
    //         //         == &sim.model
    //         //             .components
    //         //             .get(comp.model_uid as usize)
    //         //             .unwrap()
    //         //             .type_
    //         // {
    //         //     if !out_comps.contains(&comp_name.as_str().to_owned()) {
    //         //         out_comps.push(comp_name.as_str().to_owned());
    //         //     }
    //         // }
    //     }
    // // for (comp_name, var_type, var_name) in sim
    // //     .get_entity_str(wp_ent_name)
    // //     .unwrap()
    // //     .storage
    // //     .get_all_handles()
    // // {
    // //     if comp_type.as_str() == wp_comp_type && comp_name.starts_with(w) {
    // //         if !out_comps.contains(&comp_name.as_str().to_string()) {
    // //             out_comps.push(comp_name.to_string());
    // //         }
    // //     }
    // // }
    // // for comp in out_comps {
    // //     let complet = Completion {
    // //         completion: format!("/{}/{}/{}/{}", wp_ent_type, wp_ent_name, wp_comp_type, comp),
    // //         display: Some(comp.to_string()),
    // //         suffix: linefeed::Suffix::Some('/'),
    // //     };
    // //     res.push(complet);
    // // }
    // } else if word.matches("/").count() == 5 {
    //     // var type
    //     let split = word[1..].split("/").collect::<Vec<&str>>();
    //     let wp_ent_type = split[0];
    //     let wp_ent_name = split[1];
    //     let wp_comp_type = split[2];
    //     let wp_comp_name = split[3];
    //     let w = split[4];
    //
    //     // let var_types = bigworlds::VAR_TYPES;
    //     let mut out_vt = Vec::new();
    //
    //     for (comp_name, var_type, var_name) in sim
    //         .get_entity_str(wp_ent_name)
    //         .unwrap()
    //         .storage
    //         .get_all_handles()
    //     {
    //         let var_type_str = var_type.to_str().to_string();
    //         if var_type.to_str().starts_with(w)
    //             && comp_type.as_str() == wp_comp_type
    //             && comp_name.as_str() == wp_comp_name
    //         {
    //             if !out_vt.contains(&var_type_str) {
    //                 out_vt.push(var_type_str);
    //             }
    //         }
    //     }
    //     for var_type in out_vt {
    //         let complet = Completion {
    //             completion: format!(
    //                 "/{}/{}/{}/{}/{}",
    //                 wp_ent_type, wp_ent_name, wp_comp_type, wp_comp_name, var_type
    //             ),
    //             display: Some(var_type.to_string()),
    //             suffix: linefeed::Suffix::Some('/'),
    //         };
    //         res.push(complet);
    //     }
    // } else if word.matches("/").count() == 6 {
    //     // var
    //     let split = word[1..].split("/").collect::<Vec<&str>>();
    //     let wp_ent_type = split[0];
    //     let wp_ent_name = split[1];
    //     let wp_comp_type = split[2];
    //     let wp_comp_name = split[3];
    //     let wp_var_type = split[4];
    //     let w = split[5];
    //
    //     for (comp_name, var_type, var_name) in sim
    //         .get_entity_str(wp_ent_name)
    //         .unwrap()
    //         .storage
    //         .get_all_handles()
    //     {
    //         if var_name.starts_with(w)
    //             && comp_type.as_str() == wp_comp_type
    //             && comp_name.as_str() == wp_comp_name
    //             && var_type.to_str() == wp_var_type
    //         {
    //             let complet = Completion {
    //                 completion: format!(
    //                     "/{}/{}/{}/{}/{}/{}",
    //                     wp_ent_type, wp_ent_name, wp_comp_type, wp_comp_name, wp_var_type, var_name
    //                 ),
    //                 display: Some(var_name.as_str().to_string()),
    //                 suffix: linefeed::Suffix::None,
    //             };
    //             res.push(complet);
    //         }
    //     }
    // }
    // return res;
}
