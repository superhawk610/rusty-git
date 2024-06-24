use crate::object::{ObjectBuf, ObjectType};
use eyre::{Context, Result};
use std::fmt::{Debug, Display};
use std::io::BufRead;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct Commit {
    pub tree_hash: String,
    pub parent_hashes: Vec<String>,
    pub author: CommitAttribution,
    pub committer: CommitAttribution,
    pub message: String,
}

#[derive(Debug)]
pub struct CommitAttribution {
    pub name: String,
    pub email: String,
    pub timestamp: SystemTime,
}

impl Commit {
    pub fn from_buf<R>(mut object: ObjectBuf<R>) -> Result<Self>
    where
        R: BufRead + Debug,
    {
        if object.object_type != ObjectType::Commit {
            eyre::bail!("attempted to parse {} as commit", object.object_type);
        }

        let mut buf = vec![0; object.content_len];
        object.contents.read_exact(&mut buf)?;
        let s = std::str::from_utf8(&buf).context("commit should contain valid UTF-8")?;

        let mut tree_hash: Option<String> = None;
        let mut parent_hashes: Vec<String> = Vec::new();
        let mut author: Option<String> = None;
        let mut committer: Option<String> = None;

        let mut lines = s.lines();
        while let Some(line) = lines.next() {
            if line == "" {
                break;
            }

            let (t, value) = line.split_once(' ').unwrap();
            let value = value.to_owned();
            match t {
                "tree" => tree_hash = Some(value),
                "parent" => parent_hashes.push(value),
                "author" => author = Some(value),
                "committer" => committer = Some(value),
                _ => eyre::bail!("unexpected line in commit \"{value}\""),
            }
        }

        Ok(Self {
            tree_hash: tree_hash
                .ok_or_else(|| eyre::eyre!("tree must be provided"))?
                .to_owned(),
            parent_hashes,
            author: author
                .ok_or_else(|| eyre::eyre!("author must be provided"))?
                .parse()?,
            committer: committer
                .ok_or_else(|| eyre::eyre!("committer must be provided"))?
                .parse()?,
            message: lines.map(String::from).collect(),
        })
    }
}

impl CommitAttribution {
    pub fn yours_truly() -> Self {
        // FIXME: this should read from config
        Self {
            name: "Aaron Ross".into(),
            email: "superhawky610@gmail.com".into(),
            timestamp: SystemTime::now(),
        }
    }
}

#[derive(Debug)]
pub struct ParseCommitAttributionError;

impl Display for ParseCommitAttributionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("failed to parse commit attribution")
    }
}

impl std::error::Error for ParseCommitAttributionError {}

impl FromStr for CommitAttribution {
    type Err = ParseCommitAttributionError;

    fn from_str(s: &str) -> std::prelude::v1::Result<Self, Self::Err> {
        // FIXME: actually parse attribution
        Ok(Self {
            name: s.to_owned(),
            email: String::new(),
            timestamp: SystemTime::now(),
        })
    }
}

impl Display for CommitAttribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} <{}> {} +0000",
            self.name,
            self.email,
            self.timestamp.duration_since(UNIX_EPOCH).unwrap().as_secs()
        )
    }
}
