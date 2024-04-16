use crate::object::{ObjectBuf, ObjectType};
use eyre::{Context, Result};
use std::io::Read;

pub fn run(pretty: bool, object_hash: &str) -> Result<()> {
    eyre::ensure!(pretty, "only pretty-printing is supported for now");

    let mut object = ObjectBuf::read_at_hash(object_hash)?;
    match &object.object_type {
        // FIXME: move object parsing into object.rs
        ObjectType::Blob => {
            let mut buf = Vec::new();
            buf.reserve_exact(object.content_len);
            buf.resize(object.content_len, 0);
            object
                .contents
                .read_exact(&mut buf)
                .context("read blob contents")?;

            let mut overflow = vec![0];
            match object.contents.read_exact(&mut overflow) {
                Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => (),
                _ => eyre::bail!("blob contains more bytes than its content length specified"),
            }

            let mut stdout = std::io::stdout().lock();
            let mut cursor = std::io::Cursor::new(buf);
            std::io::copy(&mut cursor, &mut stdout).context("write contents to stdout")?;

            Ok(())
        }

        // tree objects delegate to `ls-tree`
        ObjectType::Tree => crate::subcommand::ls_tree::print_tree(false, object),

        // TODO: implement cat-file for commits
        object_type => eyre::bail!("unsupported object type {object_type:?}"),
    }
}
