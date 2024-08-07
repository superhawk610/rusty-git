use crate::object::ObjectHash;
use eyre::{Context, Result};
use flate2::read::ZlibDecoder;
use sha1::{Digest, Sha1};
use std::fmt::Debug;
use std::fs::File;
use std::io::{BufRead, BufReader, Cursor, Read, Seek, SeekFrom};
use std::str::FromStr;

pub struct Parser<R: BufRead> {
    inner: R,
}

impl<R: BufRead> Debug for Parser<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Parser<..>")
    }
}

#[derive(Debug)]
pub enum ParseError<Err> {
    Read(eyre::Report),
    Parse(Err),
}

/// Buffered reader over a contiguous slice of in-memory bytes.
pub type InMemoryReader = Cursor<Vec<u8>>;

/// Parser over a contiguous slice of in-memory bytes.
pub type InMemoryParser = Parser<InMemoryReader>;

impl InMemoryParser {
    pub fn get_ref(&self) -> &Vec<u8> {
        self.inner.get_ref()
    }
}

impl<R: BufRead + Debug + Seek> Seek for Parser<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

impl<R: BufRead + Debug + Seek> Parser<R> {
    /// Reset the internal cursor to the start of the content stream.
    pub fn reset(&mut self) {
        self.seek(SeekFrom::Start(0)).unwrap();
    }

    /// Skip over the next `bytes` bytes.
    pub fn skip(&mut self, bytes: usize) {
        self.seek(SeekFrom::Current(bytes as _)).unwrap();
    }
}

impl<R: BufRead + Debug> Parser<R> {
    pub fn new(reader: R) -> Self {
        Self { inner: reader }
    }

    pub fn inner_mut(&mut self) -> &mut R {
        &mut self.inner
    }

    pub fn into_inner(self) -> R {
        self.inner
    }

    pub fn parse_str(&mut self, delim: u8) -> Result<String> {
        let mut buf = Vec::new();
        self.inner
            .read_until(delim, &mut buf)
            .context("fill string from inner BufRead")?;
        let mut s = String::from_utf8(buf).context("parse string as UTF-8")?;
        let _ = s.pop(); // remove trailing delimiter
        Ok(s)
    }

    pub fn parse_str_exact<const N: usize>(&mut self) -> Result<String> {
        let mut buf = vec![0; N];
        self.inner
            .read_exact(&mut buf)
            .context("fill string from inner BufRead")?;
        let s = String::from_utf8(buf).context("parse string as UTF-8")?;
        Ok(s)
    }

    pub fn parse_usize(&mut self, delim: u8) -> Result<usize> {
        Ok(self.parse_str(delim)?.parse()?)
    }

    pub fn parse_usize_exact<const N: usize>(&mut self) -> Result<usize> {
        const USIZE_BYTES: usize = (usize::BITS / 8) as usize;
        assert!(N <= USIZE_BYTES, "must fit in usize");

        let mut buf = [0; USIZE_BYTES];
        self.inner
            .read_exact(&mut buf[(USIZE_BYTES - N)..])
            .context(format!("read {N} bytes from inner BufRead"))?;
        Ok(usize::from_be_bytes(buf))
    }

    pub fn parse_size_enc_bytes(&mut self) -> Result<Vec<u8>> {
        let mut size_bytes: Vec<u8> = Vec::new();
        loop {
            // Size encoding
            //
            // This document uses the following "size encoding" of non-negative
            // integers: From each byte, the seven least significant bits are used
            // to form the resulting integer. As long as the most significant bit
            // is 1, this process continues; the byte with MSB 0 provides the last
            // seven bits. The seven-bit chunks are concatenated. Later values are
            // more significant.
            //
            // This size encoding should not be confused with the "offset encoding",
            // which is also used in this document.
            let byte = self.read_byte()?;
            size_bytes.push(byte);
            if byte & (1 << 7) == 0 {
                // MSB was 0, which means we've reached the final bit chunk
                break;
            }
        }
        Ok(size_bytes)
    }

    pub fn parse<T: FromStr>(
        &mut self,
        delim: u8,
    ) -> std::result::Result<T, ParseError<<T as FromStr>::Err>> {
        match self.parse_str(delim) {
            Ok(str) => str.parse::<T>().map_err(ParseError::Parse),
            Err(err) => Err(ParseError::Read(err)),
        }
    }

    pub fn read_byte(&mut self) -> Result<u8> {
        let mut buf = [0; 1];
        self.inner
            .read_exact(&mut buf)
            .context("read byte from inner BufRead")?;
        Ok(buf[0])
    }

    pub fn read_bytes<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut buf = [0; N];
        self.inner
            .read_exact(&mut buf)
            .context(format!("read {N} bytes from inner BufRead"))?;
        Ok(buf)
    }

    pub fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        Ok(self.inner.read_exact(buf)?)
    }

    pub fn split_off_decode(&mut self, size: usize) -> Result<(u64, InMemoryParser)> {
        let mut buf = vec![0; size];
        let mut decoder = ZlibDecoder::new(&mut self.inner);
        decoder.read_exact(&mut buf)?;
        let consumed = decoder.total_in();
        Ok((consumed, Parser::new(Cursor::new(buf))))
    }

    pub fn at_eof(&mut self) -> Result<bool> {
        Ok(self.inner.fill_buf().context("peek contents")?.is_empty())
    }
}

impl Parser<BufReader<File>> {
    pub fn verify_checksum(mut self, file_size: usize) -> Result<(ObjectHash, Self)> {
        // verify that the 20-byte SHA-1 checksum at the end of the file
        // matches the SHA-1 of the remaining file contents (older git
        // implementations used a SHA-1 hash of the object names contained
        // within the packfile, see [here][so-packfile]).
        //
        // [so-packfile]: https://stackoverflow.com/questions/5469978/git-pack-filenames-what-is-the-digest
        self.skip(file_size - 4 - 20);
        let checksum = ObjectHash::from_bytes(&self.read_bytes::<20>()?);

        let mut f = self.into_inner().into_inner();
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
        Ok((checksum, Self::new(reader)))
    }
}
