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
    CommitTree {
        #[arg(value_name = "tree_sha")]
        object_hash: String,

        #[arg(short)]
        parent_hash: Vec<String>,

        #[arg(short)]
        message: String,
    },
    Clone {
        #[arg(value_name = "repo_url")]
        repo_url: String,

        #[arg(value_name = "dir")]
        output_dir: Option<String>,
    },
    IndexPack {
        #[arg(value_name = "packfile")]
        pack_file: String,
    },
    VerifyPack {
        #[arg(value_name = "index_file")]
        index_file: String,
    },
    UnpackObjects,
    Checkout {
        branch: String,
    },
    LsFiles {
        #[arg(short, long)]
        cached: bool,

        #[arg(short, long = "stage")]
        staged: bool,
    },
    Status,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt::init();

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
        Command::CommitTree {
            object_hash,
            parent_hash,
            message,
        } => subcommand::commit_tree::run(object_hash, parent_hash, message),
        Command::Clone {
            repo_url,
            output_dir,
        } => subcommand::clone::run(&repo_url, output_dir.as_deref()),
        Command::IndexPack { pack_file } => subcommand::index_pack::run(pack_file),
        Command::VerifyPack { index_file } => subcommand::verify_pack::run(&index_file),
        Command::UnpackObjects => subcommand::unpack_objects::run(),
        Command::Checkout { branch } => subcommand::checkout::run(&branch),
        Command::LsFiles { cached, staged } => subcommand::ls_files::run(cached, staged),
        Command::Status => subcommand::status::run(),
    }
}
