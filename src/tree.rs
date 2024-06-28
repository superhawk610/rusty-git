use crate::object::{ObjectBuf, ObjectHash, ObjectMode, ObjectType};
use eyre::{Context, Result};
use std::{fmt::Debug, io::BufRead};

#[derive(Debug)]
pub struct Tree(Vec<TreeEntry>);

#[derive(Debug)]
pub struct TreeEntry {
    pub name: String,
    pub mode: ObjectMode,
    pub hash: ObjectHash,
}

impl Tree {
    pub fn from_buf<R>(mut object: ObjectBuf<R>) -> Result<Self>
    where
        R: BufRead + Debug,
    {
        if object.object_type != ObjectType::Tree {
            eyre::bail!("attempted to parse {} as tree", object.object_type);
        }

        let mut entries = Vec::new();
        loop {
            let mode = object
                .contents
                .parse_str(b' ')
                .context("read tree entry mode")?
                .parse()
                .map_err(|s| eyre::eyre!("expected valid file mode but got {s}"))?;

            let name = object
                .contents
                .parse_str(b'\0')
                .context("read tree entry name")?;

            let mut hash_buf = [0; 20];
            object
                .contents
                .read_exact(&mut hash_buf)
                .context("read tree entry SHA")?;

            entries.push(TreeEntry {
                mode,
                name,
                hash: ObjectHash::from_bytes(&hash_buf),
            });

            // once we reach EOF, break from the loop
            if object.contents.at_eof()? {
                break;
            }
        }

        Ok(Self(entries))
    }

    pub fn entries(&self) -> &Vec<TreeEntry> {
        &self.0
    }
}
