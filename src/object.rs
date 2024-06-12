use crate::commit::Commit;
use crate::parser::{ParseError, Parser};
use eyre::{Context, Result};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use io_tee::TeeWriter;
use sha1::{Digest, Sha1};
use std::fmt::{Debug, Display};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::str::FromStr;
use tempfile::NamedTempFile;

#[derive(Debug)]
pub enum Object {
    Blob(PathBuf),
    Tree(PathBuf),
    Commit(Commit),
}

#[derive(Debug)]
pub enum ObjectMode {
    Symlink,
    Directory,
    Executable,
    Normal,
}

pub struct ObjectHash {
    hex: String,
    bin: [u8; 20],
}

impl ObjectHash {
    pub fn from_hasher(hasher: Sha1) -> Self {
        let digest = hasher.finalize();
        Self {
            hex: format!("{:x}", digest),
            bin: digest.into(),
        }
    }

    pub fn as_hex(&self) -> &str {
        &self.hex
    }

    pub fn as_bytes(&self) -> [u8; 20] {
        self.bin
    }
}

impl Display for ObjectHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.hex)
    }
}

impl Debug for ObjectHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ObjectHash<{}>", self.hex)
    }
}

impl Display for ObjectMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Symlink => write!(f, "120000"),
            Self::Directory => write!(f, "40000"),
            Self::Executable => write!(f, "100755"),
            Self::Normal => write!(f, "100644"),
        }
    }
}

impl Object {
    pub fn blob<P: Into<PathBuf>>(path: P) -> Self {
        Self::Blob(path.into())
    }

    pub fn tree<P: Into<PathBuf>>(path: P) -> Self {
        Self::Tree(path.into())
    }

    pub fn commit(commit: Commit) -> Self {
        Self::Commit(commit)
    }

    pub fn path(&self) -> &PathBuf {
        match self {
            Self::Blob(path) => path,
            Self::Tree(path) => path,
            Self::Commit(_) => panic!("attempted to call .path() on a commit object"),
        }
    }

    pub fn mode(&self) -> Result<ObjectMode> {
        let meta = self.path().metadata()?;

        Ok(if meta.is_dir() {
            ObjectMode::Directory
        } else if meta.is_symlink() {
            ObjectMode::Symlink
        } else if meta.mode() & 0o111 != 0 {
            ObjectMode::Executable
        } else {
            ObjectMode::Normal
        })
    }

    pub fn write<W: Write>(&self, mut w: W) -> Result<()> {
        match self {
            Self::Blob(path) => {
                let meta = std::fs::metadata(path).context("stat file")?;
                let mut f = File::open(path).context("open file")?;
                write!(w, "blob {}\0", meta.len())?;
                std::io::copy(&mut f, &mut w).context("hash file contents")?;

                Ok(())
            }
            Self::Tree(root) => {
                let mut objects: Vec<Object> = Vec::new();

                for f in std::fs::read_dir(root)? {
                    let f = f?;

                    // exclude .git directory
                    if f.file_name() == ".git" {
                        continue;
                    }

                    // FIXME: ignore file patterns from .gitignore
                    if f.file_name() == "target" {
                        continue;
                    }

                    if f.file_type()?.is_dir() {
                        // ignore empty directories
                        if f.path().read_dir()?.next().is_none() {
                            continue;
                        }

                        objects.push(Object::tree(f.path()));
                    } else {
                        objects.push(Object::blob(f.path()));
                    }
                }

                // TODO: figure out a more performant way to do this
                objects.sort_unstable_by_key(|obj| match &obj {
                    Object::Blob(path) => path.as_os_str().to_owned(),
                    Object::Tree(path) => {
                        let mut str = path.as_os_str().to_owned();
                        str.push("/");
                        str
                    }
                    Object::Commit(_) => unreachable!(),
                });

                let mut buf = Vec::new();

                for obj in objects {
                    write!(
                        buf,
                        "{} {}\0",
                        obj.mode()?,
                        // TODO: figure out how git handles non-UTF8 filenames
                        obj.path().file_name().unwrap().to_string_lossy()
                    )?;
                    buf.write_all(&obj.hash(true)?.as_bytes())?;
                }

                write!(w, "tree {}\0", buf.len())?;
                w.write_all(&buf).context("tree contents")?;

                Ok(())
            }
            Self::Commit(commit) => {
                let mut buf = Vec::new();

                writeln!(buf, "tree {}", commit.tree_hash)?;
                for parent_hash in commit.parent_hashes.iter() {
                    writeln!(buf, "parent {parent_hash}")?;
                }
                writeln!(buf, "author {}", commit.author)?;
                writeln!(buf, "committer {}", commit.committer)?;
                writeln!(buf, "\n{}", commit.message)?;

                write!(w, "commit {}\0", buf.len()).unwrap();
                w.write_all(&buf).context("commit contents")?;

                Ok(())
            }
        }
    }

    pub fn hash(&self, write: bool) -> Result<ObjectHash> {
        fn write_hash<W: Write>(object: &Object, mut w: W) -> Result<ObjectHash> {
            let mut hasher = Sha1::new();
            let mut writer = TeeWriter::new(&mut hasher, &mut w);
            object.write(&mut writer)?;
            Ok(ObjectHash::from_hasher(hasher))
        }

        if write {
            let mut temp = NamedTempFile::new().context("create temp file")?;
            let encoder = ZlibEncoder::new(&mut temp, flate2::Compression::default());

            let hash = write_hash(self, encoder)?;

            let prefix_dir = format!(".git/objects/{}", &hash.as_hex()[..2]);
            match std::fs::create_dir(&prefix_dir) {
                Ok(_) => (),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => (),
                err @ Err(_) => err?,
            };

            std::fs::rename(temp, format!("{}/{}", prefix_dir, &hash.as_hex()[2..]))
                .context("move temp file to .git/objects")?;

            Ok(hash)
        } else {
            write_hash(self, std::io::sink())
        }
    }
}

pub struct ObjectBuf<R: BufRead> {
    pub object_type: ObjectType,
    pub content_len: usize,
    pub contents: Parser<R>,
}

impl ObjectBuf<BufReader<ZlibDecoder<File>>> {
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
