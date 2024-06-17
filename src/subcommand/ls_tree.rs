use crate::object::{ObjectBuf, ObjectType};
use eyre::{Context, Result};
use std::fmt::Debug;
use std::io::BufRead;

#[derive(Debug)]
struct TreeEntry {
    mode: String,
    name: String,
    sha: [u8; 20],
}

pub fn run(name_only: bool, object_hash: &str) -> Result<()> {
    let object = ObjectBuf::read_at_hash(object_hash)?;
    print_tree(name_only, object)
}

pub(crate) fn print_tree<R: BufRead + Debug>(
    name_only: bool,
    mut object: ObjectBuf<R>,
) -> Result<()> {
    if object.object_type != ObjectType::Tree {
        eyre::bail!("the object specified by the given hash isn't a tree object");
    }

    // FIXME: move object parsing into object.rs
    let mut entries = Vec::new();
    loop {
        let mode = object
            .contents
            .parse_str(b' ')
            .context("read tree entry mode")?;

        let name = object
            .contents
            .parse_str(b'\0')
            .context("read tree entry name")?;

        let mut sha_buf = vec![0; 20];
        object
            .contents
            .read_exact(&mut sha_buf)
            .context("read tree entry SHA")?;

        entries.push(TreeEntry {
            mode,
            name,
            sha: sha_buf.as_slice().try_into().unwrap(),
        });

        // once we reach EOF, break from the loop
        if object.contents.at_eof()? {
            break;
        }
    }

    for entry in entries.iter() {
        if !name_only {
            let mode = &entry.mode;
            let object_type = if mode.trim_start_matches('0') == "40000" {
                "tree"
            } else {
                "blob"
            };
            print!("{mode:0>6} {object_type} ");
            for byte in entry.sha {
                print!("{byte:0>2x}");
            }
            print!("\t");
        }

        println!("{}", entry.name);
    }

    Ok(())
}
