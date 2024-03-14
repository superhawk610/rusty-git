use clap::{Parser, Subcommand};
use eyre::Result;
use rusty_git::subcommand;

#[derive(Parser, Debug)]
#[command(version)]
struct App {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Init,
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let app = App::parse();
    match &app.command {
        Command::Init => subcommand::init::run(),
    }
}
