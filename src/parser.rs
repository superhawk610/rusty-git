use eyre::{Context, Result};
use std::io::BufRead;

pub struct Parser<R: BufRead> {
    inner: R,
}

impl<R: BufRead> Parser<R> {
    pub fn new(reader: R) -> Self {
        Self { inner: reader }
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

    pub fn parse_usize(&mut self, delim: u8) -> Result<usize> {
        Ok(self.parse_str(delim)?.parse()?)
    }

    pub fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        Ok(self.inner.read_exact(buf)?)
    }

    pub fn at_eof(&mut self) -> Result<bool> {
        Ok(self.inner.fill_buf().context("peek contents")?.is_empty())
    }
}
