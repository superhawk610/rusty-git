use eyre::{Context, Result};
use flate2::read::ZlibDecoder;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::str::FromStr;

pub fn run(pretty: bool, object_hash: &str) -> Result<()> {
    eyre::ensure!(pretty, "only pretty-printing is supported for now");

    let f = File::open(format!(
        ".git/objects/{}/{}",
        &object_hash[..2],
        &object_hash[2..]
    ))
    .context("read object file")?;

    let decoder = ZlibDecoder::new(f);
    let mut decoder = BufReader::new(decoder);

    let mut buf: Vec<u8> = Vec::new();

    let _ = decoder
        .read_until(b' ', &mut buf)
        .context("read object header")?;
    let object_type = String::from_utf8(buf.clone()).context("parse object header as UTF-8")?;
    let object_type = ObjectType::from_str(&object_type[..object_type.len() - 1]);

    buf.clear();

    let _ = decoder
        .read_until(b'\0', &mut buf)
        .context("read content length")?;
    let content_len = std::ffi::CString::from_vec_with_nul(buf.clone())
        .context("parse content length as UTF-8")?;
    let content_len = content_len
        .to_str()
        .unwrap()
        .parse::<usize>()
        .context("content length is valid number")?;

    match object_type {
        Ok(ObjectType::Blob) => {
            buf.clear();
            buf.reserve_exact(content_len);
            buf.resize(content_len, 0);
            decoder.read_exact(&mut buf).context("read blob contents")?;

            let mut overflow = vec![0];
            match decoder.read_exact(&mut overflow) {
                Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => (),
                _ => eyre::bail!("blob contains more bytes than its content length specified"),
            }

            let mut stdout = std::io::stdout().lock();
            let mut cursor = std::io::Cursor::new(buf);
            std::io::copy(&mut cursor, &mut stdout).context("write contents to stdout")?;

            Ok(())
        }
        // TODO: implement cat-file for commits & trees
        Ok(object_type) => eyre::bail!("unsupported object type {object_type:?}"),
        Err(object_type) => eyre::bail!("unrecognized object type {object_type}"),
    }
}

#[derive(Debug, PartialEq)]
enum ObjectType {
    Blob,
    Commit,
    Tree,
}

impl FromStr for ObjectType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "blob" => Ok(Self::Blob),
            "commit" => Ok(Self::Commit),
            "tree" => Ok(Self::Tree),
            _ => Err(String::from(s)),
        }
    }
}
