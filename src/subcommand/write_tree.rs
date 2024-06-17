use crate::object::{Object, ObjectHashable};
use eyre::Result;

pub fn run() -> Result<()> {
    let hash = Object::tree(".").hash(true)?;

    println!("{hash}");

    Ok(())
}
