use eyre::{Context, Result};
use std::path::Path;

pub fn run() -> Result<()> {
    with_default_branch("main")
}

pub fn with_default_branch(branch: &str) -> Result<()> {
    let pwd = Path::new(".").canonicalize()?;

    if Path::new(".git").exists() {
        eprintln!("Git repository already exists in {}/.git", pwd.display());
        return Ok(());
    }

    for dir in [".git", ".git/objects", ".git/refs", ".git/refs/heads"] {
        std::fs::create_dir(dir).with_context(|| format!("create {dir} directory"))?;
    }

    std::fs::write(".git/HEAD", format!("ref: refs/heads/{}\n", branch))
        .context("create .git/HEAD")?;

    println!("Initialized Git repository in {}/.git", pwd.display());

    Ok(())
}
