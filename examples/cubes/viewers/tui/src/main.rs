#![allow(unused)]

use app::App;

mod app;
mod ui;
mod world;

const SERVER_ADDR: &'static str = "quic://127.0.0.1:9123";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_addr = parse_args();
    let mut app = App::new(server_addr.parse()?).await?;

    let mut terminal = ratatui::try_init()?;
    let res = app.run(&mut terminal).await;

    ratatui::try_restore()?;
    Ok(res?)
}

fn parse_args() -> String {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.contains(&"-h".into()) || args.contains(&"--help".into()) {
        println!(
            "TUI viewer for the cubes example.\n\n\
            USAGE: cubes-tui-viewer [server_address]\n\n\
            where [server_address] is an optional, fully-qualified\n\
            address of an existing bigworlds server running the\n\
            cubes example (default: `127.0.0.1:9123`)"
        );
        std::process::exit(0);
    }

    let server_addr = args.first().cloned().unwrap_or(SERVER_ADDR.to_owned());

    server_addr
}
