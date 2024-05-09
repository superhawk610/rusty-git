use std::{
    fmt::Display,
    time::{SystemTime, UNIX_EPOCH},
};

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
