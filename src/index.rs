use eyre::{Context, Result};
use std::fmt::{Debug, Display};
use std::io::{BufReader, BufWriter, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

use crate::object::{Object, ObjectHash, ObjectHashable};
use crate::parser::Parser;
use crate::utils::append_checksum;

pub const INDEX_HEADER: &[u8; 4] = b"DIRC";

#[derive(Debug)]
pub struct Index {
    pub version: u8,
    pub entries: Vec<IndexEntry>,
}

#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub stats: IndexEntryStats,
    pub _type: IndexEntryType,
    pub permissions: IndexEntryPermissions,
    pub hash: ObjectHash,
    pub name: String,
    pub flags: u16,
    pub flags_ext: u16,
}

#[derive(Debug, Clone)]
pub struct IndexEntryStats {
    pub ctime: u32,
    pub ctime_nsec: u32,
    pub mtime: u32,
    pub mtime_nsec: u32,
    pub dev: u32,
    pub ino: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: u32,
}

impl IndexEntryStats {
    pub fn from_metadata(meta: &std::fs::Metadata) -> Self {
        Self {
            ctime: meta.ctime() as _,
            ctime_nsec: meta.ctime_nsec() as _,
            mtime: meta.mtime() as _,
            mtime_nsec: meta.mtime_nsec() as _,
            dev: meta.dev() as _,
            ino: meta.ino() as _,
            uid: meta.uid(),
            gid: meta.gid(),
            size: meta.size() as _,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum IndexEntryType {
    RegularFile = 0b1000,
    SymbolicLink = 0b1010,
    GitLink = 0b1110,
}

#[derive(Debug)]
pub struct TryFromError<N: Display + Debug>(N);

impl<N: Display + Debug> std::error::Error for TryFromError<N> {}

impl<N: Display + Debug> Display for TryFromError<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "failed to parse {}", self.0)
    }
}

impl TryFrom<u8> for IndexEntryType {
    type Error = TryFromError<u8>;

    fn try_from(val: u8) -> std::prelude::v1::Result<Self, Self::Error> {
        match val {
            0b1000 => Ok(Self::RegularFile),
            0b1010 => Ok(Self::SymbolicLink),
            0b1110 => Ok(Self::GitLink),
            _ => Err(TryFromError(val)),
        }
    }
}

#[repr(u16)]
#[derive(Debug, Clone, Copy)]
pub enum IndexEntryPermissions {
    /// Symbolic links and gitlinks have no permissions.
    None = 0,
    RegularFile = 0o0644,
    ExecutableFile = 0o0755,
}

impl TryFrom<u16> for IndexEntryPermissions {
    type Error = TryFromError<u16>;

    fn try_from(val: u16) -> std::prelude::v1::Result<Self, Self::Error> {
        match val {
            0o0000 => Ok(Self::None),
            0o0644 => Ok(Self::RegularFile),
            0o0755 => Ok(Self::ExecutableFile),
            _ => Err(TryFromError(val)),
        }
    }
}

impl Index {
    pub fn read_default() -> Result<Self> {
        Self::read(".git/index")
    }

    pub fn read(path: impl AsRef<Path>) -> Result<Self> {
        let f = std::fs::File::open(path.as_ref()).context("open default index file")?;
        let file_size = f.metadata()?.len() as usize;

        let reader = BufReader::new(f);
        let mut parser = Parser::new(reader);

        let header = parser.read_bytes::<4>().context("read index header")?;
        if &header != INDEX_HEADER {
            eyre::bail!(
                "invalid header; expected {:?}, got {:?}",
                INDEX_HEADER,
                header
            );
        }

        let (_, mut parser) = parser.verify_checksum(file_size)?;

        let version = parser.parse_usize_exact::<4>().context("parse version")? as u8;

        let num_entries = parser
            .parse_usize_exact::<4>()
            .context("parse number of entries")? as u32;
        let mut entries = Vec::with_capacity(num_entries as _);

        let mut offset = 12; // 4 + 4 + 4
        for _ in 0..num_entries {
            let ctime = parser.parse_usize_exact::<4>().context("parse ctime")? as u32;
            let ctime_nsec = parser
                .parse_usize_exact::<4>()
                .context("parse ctime_nsec")? as u32;
            let mtime = parser.parse_usize_exact::<4>().context("parse mtime")? as u32;
            let mtime_nsec = parser
                .parse_usize_exact::<4>()
                .context("parse mtime_nsec")? as u32;
            let dev = parser.parse_usize_exact::<4>().context("parse dev")? as u32;
            let ino = parser.parse_usize_exact::<4>().context("parse ino")? as u32;

            // mode is documented as a 32 byte value, but no definition is only given for
            // the first 16 bits; only the lower 16 bits are documented...
            parser.skip(2);
            let mode = parser.parse_usize_exact::<2>().context("parse mode")? as u16;

            let uid = parser.parse_usize_exact::<4>().context("parse uid")? as u32;
            let gid = parser.parse_usize_exact::<4>().context("parse gid")? as u32;
            let size = parser.parse_usize_exact::<4>().context("parse size")? as u32;

            let hash = parser.read_bytes::<20>().context("parse object hash")?;

            let flags = parser.parse_usize_exact::<2>().context("parse flags")? as u16;

            let mut entry_len = 62;
            let flags_ext = if version >= 3
            /* && flags["extended"] */
            {
                entry_len += 2;
                todo!("parse extended flags");
            } else {
                0
            };

            let name = parser.parse_str(b'\0').context("parse name")?;
            let name_len = flags & 0x0fff;

            if name.len() <= 0x0fff && name.len() != name_len as usize {
                eyre::bail!(
                    "index entry name length mismatch; wanted {}, got {}",
                    name_len,
                    name.len()
                );
            }

            if version < 4 {
                entry_len += name.len() + 1;
                let overflow = entry_len % 8;
                let pad_bytes = if overflow == 0 { 0 } else { 8 - overflow };
                parser.skip(pad_bytes as _);
                entry_len += pad_bytes;
            }

            offset += entry_len;

            let stats = IndexEntryStats {
                ctime,
                ctime_nsec,
                mtime,
                mtime_nsec,
                dev,
                ino,
                uid,
                gid,
                size,
            };

            entries.push(IndexEntry {
                stats,
                _type: IndexEntryType::try_from(((mode & 0xf000) >> 12) as u8)
                    .context("parse entry type")?,
                permissions: IndexEntryPermissions::try_from(mode & 0x01ff)
                    .context("parse entry permissions")?,
                hash: ObjectHash::from_bytes(&hash),
                name,
                flags,
                flags_ext,
            });
        }

        loop {
            // the final 20 bytes of a packfile contain a hash of its contents,
            // which we've already verified to be correct earlier
            if offset == file_size - 20 {
                break;
            }

            let ext_header = parser.read_bytes::<4>().context("parse extension header")?;
            dbg!(std::string::String::from_utf8_lossy(&ext_header));
            let ext_size = parser
                .parse_usize_exact::<4>()
                .context("parse extension size")? as u32;
            parser.skip(ext_size as _);

            offset += 8 + ext_size as usize;
        }

        Ok(Self { version, entries })
    }

    pub fn working_tree() -> Result<Self> {
        fn entries_in_dir(path: &Path) -> Result<Vec<IndexEntry>> {
            let mut entries: Vec<IndexEntry> = Vec::new();

            // FIXME: actually read .gitignore
            let path_str = format!("{}", path.display());
            if path_str.contains(".git") || path_str.contains("target") {
                return Ok(Vec::new());
            }

            for dir_entry in std::fs::read_dir(path)? {
                let dir_entry = dir_entry?;

                if dir_entry.metadata()?.is_file() {
                    entries.push(IndexEntry::from_path(dir_entry.path())?);
                } else {
                    entries.extend(entries_in_dir(&dir_entry.path())?.into_iter());
                }
            }

            Ok(entries)
        }

        let mut entries = entries_in_dir(Path::new("."))?;
        entries.sort_unstable_by(|a, b| a.name.cmp(&b.name));

        Ok(Self {
            version: 2,
            entries,
        })
    }

    pub fn write_default(&self) -> Result<()> {
        self.write(".git/index")
    }

    pub fn write(&self, path: impl AsRef<Path>) -> Result<()> {
        let f = std::fs::File::options()
            .read(true)
            .write(true)
            .truncate(true)
            .create(true)
            .open(path.as_ref())
            .context("open or create index file")?;
        let mut writer = BufWriter::new(f);

        // 1. header
        writer.write_all(INDEX_HEADER)?;

        // 2. version
        writer.write_all(&2u32.to_be_bytes())?;

        // 3. entry count
        writer.write_all(&(self.entries.len() as u32).to_be_bytes())?;

        // 4. entries
        for entry in self.entries.iter() {
            // 4a. ctime
            writer.write_all(&entry.stats.ctime.to_be_bytes())?;
            // 4b. ctime_nsec
            writer.write_all(&entry.stats.ctime_nsec.to_be_bytes())?;
            // 4c. mtime
            writer.write_all(&entry.stats.mtime_nsec.to_be_bytes())?;
            // 4d. mtime_nsec
            writer.write_all(&entry.stats.mtime_nsec.to_be_bytes())?;
            // 4e. dev
            writer.write_all(&entry.stats.dev.to_be_bytes())?;
            // 4f. ino
            writer.write_all(&entry.stats.ino.to_be_bytes())?;
            // 4g. 2 bytes padding
            writer.write_all(&[0, 0])?;
            // 4h. mode
            writer.write_all(&((entry._type as u16) << 12).to_be_bytes())?;
            // 4i. uid
            writer.write_all(&entry.stats.uid.to_be_bytes())?;
            // 4j. gid
            writer.write_all(&entry.stats.gid.to_be_bytes())?;
            // 4k. size
            writer.write_all(&entry.stats.size.to_be_bytes())?;
            // 4l. hash
            writer.write_all(&entry.hash.as_bytes())?;
            // 4m. flags
            writer.write_all(&entry.flags.to_be_bytes())?;
            // 4n. flags_ext (v3+, skipped for version 2)
            // 4o. name
            writer.write_all(entry.name.as_bytes())?;
            writer.write_all(&[0])?;
            // 4p. padding
            let overflow = (62 + entry.name.len() + 1) % 8;
            if overflow > 0 {
                writer.write_all(&vec![0; 8 - overflow])?;
            }
        }

        // 5. extensions (skipped)

        // 6. checksum
        append_checksum(writer.into_inner()?)?;

        Ok(())
    }
}

impl IndexEntry {
    fn from_path(path: impl AsRef<Path>) -> Result<IndexEntry> {
        let path: &Path = path.as_ref();
        let f = std::fs::File::open(path)?;
        let metadata = f.metadata()?;

        let _type = if metadata.file_type().is_symlink() {
            IndexEntryType::SymbolicLink
        } else {
            IndexEntryType::RegularFile
        };

        let permissions = if metadata.file_type().is_symlink() {
            IndexEntryPermissions::None
        } else if metadata.permissions().mode() & 0o111 != 0 {
            IndexEntryPermissions::ExecutableFile
        } else {
            IndexEntryPermissions::RegularFile
        };

        let hash = Object::blob(path).hash(false)?;
        let name = format!("{}", path.display())
            .trim_start_matches("./")
            .to_owned();
        // FIXME: assume-valid, extended, stage
        let flags = if name.len() < 0xfff {
            name.len() as u16
        } else {
            0xfff_u16
        };

        Ok(Self {
            stats: IndexEntryStats::from_metadata(&metadata),
            _type,
            permissions,
            hash,
            name,
            flags,
            flags_ext: 0,
        })
    }
}
