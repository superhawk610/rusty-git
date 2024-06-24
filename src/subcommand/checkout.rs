use crate::commit::Commit;
use crate::object::ObjectBuf;
use crate::tree::Tree;
use eyre::{Context, Result};

pub fn run(branch: &str) -> Result<()> {
    // FIXME: make sure that working directory is clean first

    // TODO: check out actual files, not just `.git/objects` directory
    // 1. find tree that HEAD commit points to
    // 2. iterate through tree, copying blobs to filesystem
    // branches are actually refs; `.git/HEAD` contains something like
    // `ref: refs/heads/main`, then `.git/refs/heads/main` contains the
    // hash of the commit, which contains the tree, and so on
    let commit_hash =
        std::fs::read_to_string(format!(".git/heads/refs/{branch}")).context("read branch ref")?;
    let commit_hash = commit_hash.trim_end();

    let commit = {
        let obj = ObjectBuf::read_at_hash(commit_hash).context("read object at branch hash")?;
        Commit::from_buf(obj)?
    };

    let tree = {
        let obj = ObjectBuf::read_at_hash(&commit.tree_hash).context("read object at tree hash")?;
        Tree::from_buf(obj)?
    };
    dbg!(tree);

    panic!("oops!");

    Ok(())
}
