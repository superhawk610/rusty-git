use crate::parser::{ParseError, Parser};
use eyre::{Context, Result};
use flate2::read::ZlibDecoder;
use std::fs::File;
use std::io::BufReader;
use std::str::FromStr;

pub struct ObjectBuf {
    pub object_type: ObjectType,
    pub content_len: usize,
    pub contents: Parser<BufReader<ZlibDecoder<File>>>,
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
        let reader = BufReader::new(decoder);
        let mut parser = Parser::new(reader);

        let object_type = match parser.parse::<ObjectType>(b' ') {
            Ok(object_type) => object_type,
            Err(ParseError::Parse(object_type)) => {
                return Err(eyre::eyre!("unrecognized object type {object_type}"));
            }
            Err(ParseError::Read(err)) => {
                return Err(err);
            }
        };

        let content_len = parser.parse_usize(b'\0').context("content length")?;

        Ok(Self {
            object_type,
            content_len,
            contents: parser,
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
