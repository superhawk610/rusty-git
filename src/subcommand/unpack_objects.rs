use eyre::Result;
use std::io::{BufRead, BufReader};

pub fn run() -> Result<()> {
    let mut stdin = BufReader::new(std::io::stdin().lock());

    // read 1 or more packfiles from stdin and unpack them to loose objects
    todo!();

    Ok(())
}
