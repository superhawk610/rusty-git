use crate::pack::Pack;
use eyre::Result;
use std::path::Path;

/// Given a `.idx` index file, verify that the corresponding packfile exists and is well formed.
pub fn run(index_file: &str) -> Result<()> {
    let index_file: &Path = index_file.as_ref();

    let pack = Pack::open_index(index_file)?;
    dbg!(pack);

    Ok(())
}
