use std::str::FromStr;

use bigworlds::{client::r#async::AsyncClient, Address};
use messageio_client::Client;

use crate::interactive::config::Config;

pub async fn process_step(client: &mut Client, config: &Config) -> anyhow::Result<()> {
    client.step_request(config.turn_steps as u32).await?;
    Ok(())
}

/// Create the prompt string. It defaults to current clock tick integer number.
/// It can display a custom prompt based on the passed configuration.
pub async fn create_prompt(client: &mut Client, cfg: &Config) -> anyhow::Result<String> {
    if &cfg.prompt_format == "" {
        return create_prompt_default(client).await;
    }

    let mut var_addrs = Vec::new();
    for v in &cfg.prompt_vars {
        let addr = match Address::from_str(v) {
            Ok(a) => a,
            Err(e) => {
                return create_prompt_default(client).await;
            }
        };
        var_addrs.push(addr.to_string());
    }
    // println!("create prompt end");
    unimplemented!();
    // let vars = client.get_vars_as_strings(&var_addrs).unwrap();
    // let matches: Vec<&str> = cfg.prompt_format.matches("{}").collect();
    // if matches.len() != vars.len() {
    //     return create_prompt_default(client);
    // }
    // let mut out_string = format!("[{}] ", cfg.prompt_format.clone());
    // for var_res in vars {
    //     out_string = out_string.replacen("{}", &var_res, 1);
    // }
    // Ok(out_string)
}

pub async fn create_prompt_default(client: &mut Client) -> anyhow::Result<String> {
    let status = client.status().await?;
    let clock = status.current_tick;
    Ok(format!(
        "[{}] ",
        //TODO this should instead ask for a default clock variable
        // that's always available (right now it's using clock mod's var)
        clock,
    ))
}

fn print_show(client: &mut Client, config: &Config) {
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

        // TODO
        let addr = match Address::from_str(addr_str) {
            Ok(a) => a,
            Err(_) => continue,
        };
        unimplemented!();
        // let val = client.get_var_as_string(&addr.to_string()).unwrap();

        // println!("{}{}{}", addr_str, diff, val);
    }
}
pub fn print_show_grid(client: &mut Client, config: &Config, addr_str: &str, multiplier: f32) {
    unimplemented!()
    // use image::GenericImage;
    // let grid = client
    //     // .get_var(&Address::from_str(addr_str).expect("failed making
    // addr"))     .get_var(addr_str)
    //     .expect("failed getting grid");
    // let grid = grid.as_grid().expect("var is not grid");
    // println!("{}", grid.len());
    // let mut img = image::DynamicImage::new_bgr8(grid[0].len() as u32,
    // grid.len() as u32); for (line_count, line) in grid.iter().enumerate()
    // {     for (pix_count, pix) in line.iter().enumerate() {
    //         let pix8 = (pix.to_float() * multiplier as f32) as u8;
    //         // let pix8 = *pix as u8;
    //         img.put_pixel(
    //             pix_count as u32,
    //             line_count as u32,
    //             image::Rgba([pix8, pix8, pix8, 255]),
    //         );
    //     }
    // }
    // super::img_print::print_image(img, true, 100, 50);
}
