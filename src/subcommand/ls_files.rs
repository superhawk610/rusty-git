use crate::index::{Index, IndexEntryPermissions};
use eyre::{Context, Result};

pub fn run(cached: bool, staged: bool) -> Result<()> {
    let index = Index::read_default().context("read index")?;

    for entry in index.entries.iter() {
        if staged {
            let mode = match &entry.permissions {
                IndexEntryPermissions::None => "000000",
                IndexEntryPermissions::RegularFile => "100644",
                IndexEntryPermissions::ExecutableFile => "100755",
            };

            print!("{} {} {}\t", mode, entry.hash, entry.flags & 0x3000);
        }

        println!("{}", entry.name);
    }

    Ok(())
}
