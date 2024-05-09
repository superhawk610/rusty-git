use crate::object::Object;
use eyre::Result;

pub fn run(write: bool, path: &str) -> Result<()> {
    let hash = Object::blob(path).hash(write)?;

    println!("{hash}");

    Ok(())
}
