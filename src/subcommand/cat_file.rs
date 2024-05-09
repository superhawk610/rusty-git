use crate::object::{ObjectBuf, ObjectType};
use eyre::{Context, Result};

pub fn run(pretty: bool, object_hash: &str) -> Result<()> {
    eyre::ensure!(pretty, "only pretty-printing is supported for now");

    let mut object = ObjectBuf::read_at_hash(object_hash)?;
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

        // TODO: implement cat-file for commits
        object_type => eyre::bail!("unsupported object type {object_type:?}"),
    }
}
