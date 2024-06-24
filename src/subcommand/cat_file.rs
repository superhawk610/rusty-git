use crate::object::{ObjectBuf, ObjectType};
use eyre::{Context, Result};
use std::fmt::Debug;
use std::io::BufRead;

pub fn run(pretty: bool, object_hash: &str) -> Result<()> {
    eyre::ensure!(pretty, "only pretty-printing is supported for now");

    let object = ObjectBuf::read_at_hash(object_hash)?;
    print_obj(object)?;

    Ok(())
}

pub fn print_obj<R: BufRead + Debug>(mut object: ObjectBuf<R>) -> Result<()> {
    match &object.object_type {
        // FIXME: move object parsing into object.rs
        ObjectType::Blob => {
            let mut buf = vec![0; object.content_len];

            object
                .contents
                .read_exact(&mut buf)
                .context("read blob contents")?;

            if !object.contents.at_eof()? {
                eyre::bail!("blob contains more bytes than its content length specified");
            }

            let mut stdout = std::io::stdout().lock();
            let mut cursor = std::io::Cursor::new(buf);
            std::io::copy(&mut cursor, &mut stdout).context("write contents to stdout")?;

            Ok(())
        }

        // tree objects delegate to `ls-tree`
        ObjectType::Tree => crate::subcommand::ls_tree::print_tree(false, object),

        ObjectType::Commit | ObjectType::Tag => {
            let mut buf = vec![0; object.content_len];
            object.contents.read_exact(&mut buf)?;
            println!("{}", String::from_utf8_lossy(&buf));

            Ok(())
        }
    }
}
