use crate::index::Index;
use eyre::{Context, Result};

pub fn run() -> Result<()> {
    let index = Index::read_default().context("read index")?;

    // TODO: compare index to working tree
    dbg!(index);

    Ok(())
}
