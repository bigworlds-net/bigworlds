use std::str::FromStr;

use anyhow::Result;

use bigworlds::Address;
use bigworlds::SimHandle;

use crate::interactive::config::Config;

/// Create the prompt string. It defaults to current clock tick integer number.
/// It can display a custom prompt based on the configuration file.
///
/// ### Example
///
/// Example of custom prompt setup (interactive.yaml):
///
/// ```yaml
/// prompt_format: "{}-{}-{} {}:00"
/// prompt_vars: [
/// 	"/singleton/clock/str/day",
/// 	"/singleton/clock/str/month",
/// 	"/singleton/clock/str/year",
/// 	"/singleton/clock/str/hour",
/// ]
/// ```
pub async fn create_prompt(sim: &SimHandle, cfg: &Config) -> String {
    //    println!("create prompt: {}", cfg.prompt_format.clone());
    if &cfg.prompt_format == "" {
        return create_prompt_default(sim).await;
    }
    // vars resolved
    let mut vars_res = Vec::new();
    for v in &cfg.prompt_vars {
        let addr = match Address::from_str(v) {
            Ok(a) => a,
            Err(e) => {
                return create_prompt_default(sim).await;
            }
        };
        let var_res = match sim.get_var(addr).await.unwrap().as_string() {
            Ok(i) => i.clone(),
            Err(_) => {
                return create_prompt_default(sim).await;
            }
        };
        vars_res.push(var_res);
    }
    let matches: Vec<&str> = cfg.prompt_format.matches("{}").collect();
    //    println!("{}", matches.len());
    if matches.len() != vars_res.len() {
        return create_prompt_default(sim).await;
    }
    let mut out_string = format!("[{}] ", cfg.prompt_format.clone());
    for var_res in vars_res {
        out_string = out_string.replacen("{}", &var_res, 1);
    }
    out_string
}
async fn create_prompt_default(sim: &SimHandle) -> String {
    format!("[{}] ", get_sim_clock_string(sim).await)
}
async fn get_sim_clock_string(sim: &SimHandle) -> String {
    format!("{}", sim.get_clock().await.unwrap())
}

pub fn print_show_grid(sim: &SimHandle, config: &Config, addr_str: &str) {
    unimplemented!();
}

// #[cfg(all(feature = "grids", feature = "img_print"))]
// pub async fn print_show_grid(sim: &SimHandle, config: &Config, addr_str: &str) {
//     use image::GenericImage;
//     let grid = sim
//         .get_var(Address::from_str(addr_str).expect("failed making addr"))
//         .await
//         .expect("failed getting grid")
//         .as_grid()
//         .expect("var is not grid");
//     println!("{}", grid.len());
//     let mut img = image::DynamicImage::new_rgb8(grid[0].len() as u32, grid.len() as u32);
//     for (line_count, line) in grid.iter().enumerate() {
//         for (pix_count, pix) in line.iter().enumerate() {
//             let pix8 = pix.to_float() as u8;
//             // let pix8 = *pix as u8;
//             img.put_pixel(
//                 pix_count as u32,
//                 line_count as u32,
//                 image::Rgba([pix8, pix8, pix8, 255]),
//             );
//         }
//     }
//     super::img_print::print_image(img, true, 100, 50);
// }
pub async fn print_show(sim: &SimHandle, config: &Config) {
    let mut longest_addr: usize = 0;
    for addr_str in &config.show_list {
        if addr_str.len() > longest_addr {
            longest_addr = addr_str.len();
        }
    }
    for addr_str in &config.show_list {
        // slightly convoluted way of getting two neat columns
        let len_diff = longest_addr - addr_str.len() + 6;
        let mut v = Vec::new();
        for i in 0..len_diff {
            v.push(' ')
        }
        let diff: String = v.into_iter().collect();

        // TODO hack
        let addr = match Address::from_str(addr_str) {
            Ok(a) => a,
            Err(_) => continue,
        };
        if let Ok(v) = sim.get_var(addr).await {
            if let Ok(_v) = v.as_string() {
                println!("{}{}{}", addr_str, diff, _v);
            } else {
                continue;
            }
        } else {
            continue;
        }
    }
}

/// Processes a sequence of steps called a *turn*. Turn can be technically
/// any integer bigger than 0.
pub async fn process_turn(sim: &mut SimHandle, config: &Config) -> Result<()> {
    let turn_steps: i32 = config.get("turn_steps").unwrap().parse().unwrap();
    for n in 0..turn_steps {
        sim.step().await?;
    }

    if config.show_on {
        print_show(&sim, config).await;
    }

    Ok(())
}
