use eyre::{Context, Result};
use std::fs;
use std::path::Path;

pub fn run() -> Result<()> {
    let pwd = Path::new(".").canonicalize()?;
    let git_dir = Path::new(".git");
    if git_dir.exists() {
        eprintln!("Git repository already exists in {}/.git", pwd.display());
        return Ok(());
    }

    fs::create_dir(".git").context("create .git directory")?;
    fs::create_dir(".git/objects").context("create .git/objects")?;
    fs::create_dir(".git/refs").context("create .git/refs")?;
    fs::write(".git/HEAD", b"ref: refs/heads/main\n").context("create .git/HEAD")?;

    eprintln!("Initialized Git repository in {}/.git", pwd.display());

    Ok(())
}
