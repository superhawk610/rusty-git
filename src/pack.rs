use crate::object::{ObjectBuf, ObjectHash, ObjectHashable, ObjectType};
use crate::parser::{InMemoryReader, Parser};
use eyre::{Context, Result};
use sha1::{Digest, Sha1};
use std::fmt::Debug;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

#[derive(Debug)]
pub struct Pack {
    version: u32,
    obj_count: u32,
    checksum: ObjectHash,

    /// Compressed contents of the pack file; these are kept in order by their hashes.
    contents: Vec<PackedObject>,
}

#[derive(Debug)]
pub struct PackedObject {
    /// The hash of the object this entry contains.
    pub hash: ObjectHash,
    /// The decompressed size of this object's content.
    pub size: usize,
    /// The byte offset of this pack in the containing file.
    pub offset: usize,
    pub inner: PackedObjectInner,
}

#[derive(Debug)]
pub enum PackedObjectInner {
    // 0 is invalid, 5 is reserved

    // OBJ_COMMIT (1)
    // OBJ_TREE (2)
    // OBJ_BLOB (3)
    Object(ObjectBuf<InMemoryReader>),

    // OBJ_TAG (4)
    // TODO: figure out how tags are stored

    // OBJ_OFS_DELTA (6)
    // OBJ_REF_DELTA (7)
    Delta {
        _type: DeltaType,
        base_hash: String,
        instructions: Vec<DeltaInstruction>,
    },
}

#[derive(Debug)]
pub enum DeltaInstruction {
    /// Copy `size` bytes from the base object, starting at `offset`.
    Copy { offset: usize, size: usize },

    /// Append the contained bytes to the end of the object.
    Add(Vec<u8>),
}

#[derive(Debug)]
pub enum DeltaType {
    /// Relative offset to another object within the same packfile.
    Offset,

    /// Hash for another object that may or may not be in the same packfile.
    Reference,
}

impl Pack {
    /// Open a packfile that does *not* have an index.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let f = File::open(path.as_ref()).context("open packfile")?;
        let file_size = f.metadata()?.len() as usize;
        let reader = BufReader::new(f);
        let mut parser = Parser::new(reader);

        // first, verify that the magic header is present and well-formed
        let header = parser.parse_str_exact::<4>()?;
        if header != "PACK" {
            eyre::bail!("malformed packfile header");
        }

        // second, verify that the 20-byte SHA-1 checksum at the end
        // of the file matches the SHA-1 of the remaining file contents
        // (older git implementations used a SHA-1 hash of the object
        // names contained within the packfile, see [here][so-packfile].
        //
        // [so-packfile]: https://stackoverflow.com/questions/5469978/git-pack-filenames-what-is-the-digest
        parser.skip(file_size - 4 - 20)?;
        let checksum = ObjectHash::from_bytes(&parser.read_bytes::<20>()?);

        let mut f = parser.into_inner().into_inner();
        f.seek(SeekFrom::Start(0)).unwrap();
        let mut hasher = Sha1::new();
        std::io::copy(
            &mut f.try_clone().unwrap().take(file_size as u64 - 20),
            &mut hasher,
        )?;
        let sha1 = ObjectHash::from_hasher(hasher);

        if sha1 != checksum {
            eyre::bail!("checksums don't match (wanted {}, got {})", checksum, sha1);
        }

        f.seek(SeekFrom::Start(4)).unwrap();
        let reader = BufReader::new(f);
        parser = Parser::new(reader);

        let version = parser
            .parse_usize_exact::<4>()
            .context("parse packfile version")? as u32;

        let obj_count = parser
            .parse_usize_exact::<4>()
            .context("parse packfile object count")? as u32;

        let mut pack_contents = Vec::new();

        let mut offset: usize = 12; // 4 + 4 + 4
        loop {
            // the final 20 bytes of a packfile contain a hash of its contents,
            // which we've already verified to be correct earlier
            if offset == file_size - 20 {
                break;
            }

            // 1 0 0 1 1 1 1 0   0 0 0 0 1 1 1 1
            // ^ |-t-| |--A--|   ^ |-----B-----|
            //
            // the MSB of each byte tells whether to continue parsing (variable len encoding)
            //
            // the first 3 bits of the result indicate the type (see below); the remaining
            // bits should be concatenated, in reverse order (A is the low bits, B is high),
            // to form the actual value: 0b1111_1110
            let size_bytes = parser.parse_size_enc_bytes()?;
            offset += size_bytes.len();

            // Valid object types are:
            //
            //   - OBJ_COMMIT (1)
            //   - OBJ_TREE (2)
            //   - OBJ_BLOB (3)
            //   - OBJ_TAG (4)
            //   - OBJ_OFS_DELTA (6)
            //   - OBJ_REF_DELTA (7)
            //
            // Type 5 is reserved for future expansion. Type 0 is invalid.
            let obj_type = (size_bytes[0] & 0b0111_0000) >> 4;

            let mut size: usize = (size_bytes[0] & 0b0000_1111) as usize;
            size = size_enc_init(&size_bytes[1..], size, 4);

            let consumed = match obj_type {
                0 => eyre::bail!("invalid object type (invalid)"),

                1..=3 => {
                    let (consumed, contents) = parser.split_off_decode(size)?;

                    let mut object = ObjectBuf {
                        object_type: match obj_type {
                            1 => ObjectType::Commit,
                            2 => ObjectType::Tree,
                            3 => ObjectType::Blob,
                            _ => unreachable!("only 1..=3 available in parent match"),
                        },
                        content_len: size,
                        contents,
                    };

                    let hash = object.hash(false).context("hash object contents")?;
                    object.contents = object.contents.reset();

                    pack_contents.push(PackedObject {
                        hash,
                        size,
                        offset,
                        inner: PackedObjectInner::Object(object),
                    });

                    consumed as usize
                }

                // TODO: figure out how to display/store tags
                4 => todo!("tag encoding"),

                5 => eyre::bail!("invalid object type (reserved)"),

                // TODO: figure out OFS encoding
                // OFS delta encodes the offset of the object in the pack
                6 => todo!("OFS delta encoding"),

                // REF delta uses the object's hash
                7 => {
                    let hash = parser.read_bytes::<20>()?;
                    print!("object hash: ");
                    for byte in hash.iter() {
                        print!("{byte:x}");
                    }
                    println!();

                    let (consumed, mut contents) = parser.split_off_decode(size)?;

                    let size_base_bytes = contents.parse_size_enc_bytes()?;
                    dbg!(&size_base_bytes);
                    print!("size_bash_bytes hash: ");
                    for byte in size_base_bytes.iter() {
                        print!("{byte:08b} ");
                    }
                    println!();
                    let size_base = size_enc(&size_base_bytes);
                    dbg!(size_base);

                    let size_new_bytes = contents.parse_size_enc_bytes()?;
                    dbg!(&size_new_bytes);
                    let size_new = size_enc(&size_new_bytes);
                    dbg!(size_new);

                    // TODO: parse instruction + apply until contents is consumed

                    (consumed as usize) + 20 // hash length
                }

                _ => eyre::bail!("invalid object type (out of range)"),
            };

            offset += consumed;

            // Reset the file offset to the start of the next entry, or the checksum
            // if we've just finished parsing the final object entry. This is required
            // because `ZlibDecoder` is greedy and will pull in more bytes than it needs
            // to decode the contents, including some of the subsequent entry.
            let mut f = parser.into_inner().into_inner();
            f.seek(SeekFrom::Start(offset as _)).expect("valid offset");
            let reader = BufReader::new(f);
            parser = Parser::new(reader);
        }

        // make sure pack contents are kept in ascending order by object hash
        pack_contents.sort_by_key(|obj| obj.hash.as_bytes());

        Ok(Self {
            version,
            obj_count,
            checksum,
            contents: pack_contents,
        })
    }

    /// Open the packfile pointed to by the given index.
    pub fn open_index(path: impl AsRef<Path>) -> Result<Self> {
        todo!()
    }

    pub fn write_index(&self, path: impl AsRef<Path>) -> Result<()> {
        let f = File::options()
            .read(true)
            .write(true)
            .truncate(true)
            .create(true)
            .open(path.as_ref())
            .context("create index file")?;
        let mut writer = BufWriter::new(f);

        // We'll always write a version 2 [index][1] file.
        //
        // [1]: https://git-scm.com/docs/gitformat-pack#_version_2_pack_idx_files_support_packs_larger_than_4_gib_and

        // 1. magic number
        const IDX_MAGIC_NUM: [u8; 4] = [0xff, 0x74, 0x4f, 0x63];
        writer.write_all(&IDX_MAGIC_NUM)?;

        // 2. version number
        const IDX_VERSION: u32 = 2;
        writer.write_all(&IDX_VERSION.to_be_bytes())?;

        // 3. (layer 1) first-level fan-out table
        //
        // The header consists of 256 4-byte network byte order integers.
        // N-th entry of this table records the number of objects in the
        // corresponding pack, the first byte of whose object name is less
        // than or equal to N. This is called the first-level fan-out table.
        let mut fan_out = FanOutTable::new();
        for obj in self.contents.iter() {
            fan_out.add(&obj.hash);
        }
        for freq in fan_out.cum_freq() {
            writer.write_all(&freq.to_be_bytes())?;
        }

        // 4. (layer 2) table of sorted object names
        for obj in self.contents.iter() {
            writer.write_all(&obj.hash.as_bytes())?;
        }

        // 5. (layer 3) table of cyclic redundancy check (CRC32) values
        //
        // Since packfiles are optimized for usage across a network, these
        // check values allow us to verify that the pack's contents are valid.
        for obj in self.contents.iter() {
            // FIXME: calculate and write actual CRC32 values
            writer.write_all(&[0, 0, 0, 0])?;
        }

        // 6. (layer 4) packfile offsets
        let mut large_offsets: Vec<u64> = Vec::new();
        for obj in self.contents.iter() {
            // MSB is reserved for indicating whether this is an offset value
            // in the packfile (MSB = 0), or an offset into layer 5 (MSB = 1)
            if obj.offset <= 0x7f_ff_ff_ff {
                writer.write_all(&obj.offset.to_be_bytes())?;
            } else {
                assert!(
                    large_offsets.len() < 0x7f_ff_ff_ff,
                    "can only store {} large offsets",
                    0x7f_ff_ff_ff
                );

                let layer_5_index = 0x80_00_00_00 & (large_offsets.len() as u32);
                large_offsets.push(obj.offset as u64);
                writer.write_all(&layer_5_index.to_be_bytes())?;
            }
        }

        // 7. (layer 5) extended packfile offsets (only present in packfiles > 2GB)
        for offset in large_offsets {
            writer.write_all(&offset.to_be_bytes())?;
        }

        // 8. packfile checksum
        writer.write_all(&self.checksum.as_bytes())?;

        // 9. index file checksum
        let mut f = writer.into_inner()?;
        f.seek(SeekFrom::Start(0)).unwrap();
        let mut hasher = Sha1::new();
        std::io::copy(&mut f, &mut hasher)?;
        let index_checksum = ObjectHash::from_hasher(hasher);
        f.write_all(&index_checksum.as_bytes())?;

        Ok(())
    }
}

/// A table storing the cumulative frequency of hashes in a set that begin
/// with a byte less than or equal to the current index. Hashes are assumed
/// to be unique; this must be enforced by the caller.
struct FanOutTable {
    inner: [u32; 256],
    size: u32,
}

impl FanOutTable {
    pub fn new() -> Self {
        Self {
            inner: [0; 256],
            size: 0,
        }
    }

    pub fn add(&mut self, hash: &ObjectHash) {
        assert!(
            self.size < u32::MAX,
            "fan-out table can only store {} entries",
            u32::MAX
        );

        let first_byte = hash.as_bytes()[0];
        self.inner[first_byte as usize] += 1;
    }

    /// Return the cumulative frequency of all hashes added to the set.
    pub fn cum_freq(&self) -> impl Iterator<Item = u32> + '_ {
        let mut i = 0;
        let mut sum = 0;
        std::iter::from_fn(move || {
            if i < self.inner.len() - 1 {
                sum += self.inner[i];
                i += 1;
                Some(sum)
            } else {
                None
            }
        })
    }
}

fn size_enc(size_bytes: &[u8]) -> usize {
    size_enc_init(size_bytes, 0, 0)
}

fn size_enc_init(size_bytes: &[u8], init_n: usize, init_shift: usize) -> usize {
    let mut n = init_n;
    let mut shift = init_shift;

    for byte in size_bytes {
        n += ((byte & 0b0111_1111) as usize) << shift;
        shift += 7;
    }

    n
}
