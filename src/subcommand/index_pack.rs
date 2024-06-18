use crate::pack::Pack;
use eyre::Result;
use std::path::Path;

/// Given a `.pack` packfile, create a corresponding `.idx` index file that maps its contents.
pub fn run(pack_file: impl AsRef<Path>) -> Result<()> {
    let pack_file: &Path = pack_file.as_ref();
    let index_file = pack_file.with_extension("idx");

    let pack = Pack::open(pack_file)?;
    pack.write_index(index_file)?;
    println!("{}", pack.checksum);

    Ok(())
}
