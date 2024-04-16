use crate::object::{ObjectBuf, ObjectType};
use eyre::{Context, Result};
use std::{
    ffi::CString,
    io::{BufRead, Read},
};

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

pub(crate) fn print_tree(name_only: bool, mut object: ObjectBuf) -> Result<()> {
    if object.object_type != ObjectType::Tree {
        eyre::bail!("the object specified by the given hash isn't a tree object");
    }

    // FIXME: move object parsing into object.rs
    let mut entries = Vec::new();
    loop {
        let mut mode = Vec::new();
        object
            .contents
            .read_until(b' ', &mut mode)
            .context("read tree entry mode")?;
        let mut mode = String::from_utf8(mode).context("parse tree entry mode as UTF-8")?;
        let _ = mode.pop(); // remove trailing space

        let mut name = Vec::new();
        object
            .contents
            .read_until(b'\0', &mut name)
            .context("read tree entry name")?;
        let name = CString::from_vec_with_nul(name).unwrap();

        let mut sha_buf = vec![0; 20];
        object
            .contents
            .read_exact(&mut sha_buf)
            .context("read tree entry SHA")?;

        entries.push(TreeEntry {
            mode,
            name: name
                .into_string()
                .context("parse tree entry name as UTF-8")?,
            sha: sha_buf.as_slice().try_into().unwrap(),
        });

        // once we reach EOF, break from the loop
        if object
            .contents
            .fill_buf()
            .context("peek tree contents")?
            .is_empty()
        {
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
