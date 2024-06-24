use crate::object::{ObjectBuf, ObjectType};
use eyre::Result;
use std::{fmt::Debug, io::BufRead};

#[derive(Debug)]
pub struct Tree;

impl Tree {
    pub fn from_buf<R>(object: ObjectBuf<R>) -> Result<Self>
    where
        R: BufRead + Debug,
    {
        if object.object_type != ObjectType::Tree {
            eyre::bail!("attempted to parse {} as tree", object.object_type);
        }

        // FIXME: probably want to move most of crate::subcommand::ls_tree::print_tree here
        todo!()
    }
}
