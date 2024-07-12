use crate::object::ObjectHash;
use eyre::Result;
use sha1::{Digest, Sha1};
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};

/// Given a file, calculate the SHA-1 checksum for its contents and append it to the end.
pub fn append_checksum(mut f: File) -> Result<()> {
    f.seek(SeekFrom::Start(0)).unwrap();
    let mut hasher = Sha1::new();
    std::io::copy(&mut f, &mut hasher)?;
    let index_checksum = ObjectHash::from_hasher(hasher);
    f.write_all(&index_checksum.as_bytes())?;

    Ok(())
}
