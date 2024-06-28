use crate::object::ObjectBuf;
use crate::tree::Tree;
use crate::{commit::Commit, object::ObjectType};
use eyre::{Context, Result};
use std::path::PathBuf;

// FIXME: make sure that working directory is clean first
pub fn run(branch: &str) -> Result<()> {
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

    unpack_in(PathBuf::from("."), &tree).context("check out file contents")?;

    Ok(())
}

fn unpack_in(root: PathBuf, tree: &Tree) -> Result<()> {
    for entry in tree.entries() {
        let mut obj = ObjectBuf::read_at_hash(entry.hash.as_hex())?;
        match obj.object_type {
            ObjectType::Blob => {
                let mut f = std::fs::File::create(root.join(&entry.name))?;
                std::io::copy(obj.contents.inner_mut(), &mut f)?;
            }
            ObjectType::Tree => {
                let tree = Tree::from_buf(obj)?;
                let sub_root = root.join(&entry.name);
                std::fs::create_dir(&sub_root)?;
                unpack_in(sub_root, &tree)?;
            }
            _ => unreachable!("trees can only contain blobs and trees"),
        }
    }

    Ok(())
}
