use eyre::{Context, Result};
use flate2::write::ZlibEncoder;
use io_tee::TeeWriter;
use sha1::{Digest, Sha1};
use std::fs::File;
use std::io::Write;
use tempfile::NamedTempFile;

pub fn run(write: bool, path: &str) -> Result<()> {
    fn hash<W: Write>(path: &str, mut w: W) -> Result<String> {
        let meta = std::fs::metadata(path).context("stat file")?;
        let mut f = File::open(path).context("open file")?;

        let mut hasher = Sha1::new();
        let mut writer = TeeWriter::new(&mut hasher, &mut w);
        write!(writer, "blob {}\0", meta.len()).unwrap();
        std::io::copy(&mut f, &mut writer).context("hash file contents")?;

        Ok(format!("{:x}", hasher.finalize()))
    }

    let hash = if write {
        let mut temp = NamedTempFile::new().context("create temp file")?;
        let encoder = ZlibEncoder::new(&mut temp, flate2::Compression::default());

        let hash = hash(path, encoder)?;

        let prefix_dir = format!(".git/objects/{}", &hash[..2]);
        match std::fs::create_dir(&prefix_dir) {
            Ok(_) => (),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => (),
            err @ Err(_) => err?,
        };

        std::fs::rename(temp, format!("{}/{}", prefix_dir, &hash[2..]))
            .context("move temp file to .git/objects")?;

        hash
    } else {
        hash(path, std::io::sink())?
    };

    println!("{hash}");

    Ok(())
}
