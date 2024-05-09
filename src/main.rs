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
    CatFile {
        #[arg(short)]
        pretty: bool,

        #[arg(value_name = "object")]
        object_hash: String,
    },
    HashObject {
        #[arg(short)]
        write: bool,

        path: String,
    },
    LsTree {
        #[arg(value_name = "tree_sha")]
        object_hash: String,

        #[arg(long)]
        name_only: bool,
    },
    WriteTree,
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let app = App::parse();
    match app.command {
        Command::Init => subcommand::init::run(),
        Command::CatFile {
            pretty,
            object_hash,
        } => subcommand::cat_file::run(pretty, &object_hash),
        Command::HashObject { write, path } => subcommand::hash_object::run(write, &path),
        Command::LsTree {
            object_hash,
            name_only,
        } => subcommand::ls_tree::run(name_only, &object_hash),
        Command::WriteTree => subcommand::write_tree::run(),
    }
}
