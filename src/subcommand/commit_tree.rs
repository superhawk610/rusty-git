use crate::commit::{Commit, CommitAttribution};
use crate::object::{Object, ObjectHashable};
use eyre::Result;

pub fn run(tree_hash: String, parent_hashes: Vec<String>, message: String) -> Result<()> {
    let commit = Commit {
        tree_hash,
        parent_hashes,
        author: CommitAttribution::yours_truly(),
        committer: CommitAttribution::yours_truly(),
        message,
    };

    let hash = Object::commit(commit).hash(true)?;

    println!("{hash}");

    Ok(())
}
