use crate::object::{ObjectBuf, ObjectMode, ObjectType};
use crate::tree::Tree;
use eyre::Result;
use std::fmt::Debug;
use std::io::BufRead;

pub fn run(name_only: bool, object_hash: &str) -> Result<()> {
    let object = ObjectBuf::read_at_hash(object_hash)?;
    print_tree(name_only, object)
}

pub(crate) fn print_tree<R: BufRead + Debug>(name_only: bool, object: ObjectBuf<R>) -> Result<()> {
    if object.object_type != ObjectType::Tree {
        eyre::bail!("the object specified by the given hash isn't a tree object");
    }

    for entry in Tree::from_buf(object)?.entries().iter() {
        if !name_only {
            let object_type = if entry.mode == ObjectMode::Directory {
                "tree"
            } else {
                "blob"
            };
            print!("{:0>6} {} {}\t", entry.mode, object_type, entry.hash);
        }

        println!("{}", entry.name);
    }

    Ok(())
}
