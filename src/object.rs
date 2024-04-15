use eyre::{Context, Result};
use flate2::read::ZlibDecoder;
use std::ffi::CString;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::str::FromStr;

pub struct ObjectBuf {
    pub object_type: ObjectType,
    pub content_len: usize,
    pub contents: BufReader<ZlibDecoder<File>>,
}

impl ObjectBuf {
    pub fn read_at_hash(object_hash: &str) -> Result<Self> {
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
        let object_type = match ObjectType::from_str(&object_type[..object_type.len() - 1]) {
            Ok(object_type) => object_type,
            Err(object_type) => {
                return Err(eyre::eyre!("unrecognized object type {object_type}"));
            }
        };

        buf.clear();

        let _ = decoder
            .read_until(b'\0', &mut buf)
            .context("read content length")?;
        let content_len =
            CString::from_vec_with_nul(buf.clone()).context("parse content length as UTF-8")?;
        let content_len = content_len
            .to_str()
            .unwrap()
            .parse::<usize>()
            .context("content length is valid number")?;

        Ok(Self {
            object_type,
            content_len,
            contents: decoder,
        })
    }
}

#[derive(Debug, PartialEq)]
pub enum ObjectType {
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
