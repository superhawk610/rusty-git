use crate::object::Object;
use eyre::Result;

pub fn run() -> Result<()> {
    let hash = Object::tree(".").hash(true)?;

    println!("{hash}");

    Ok(())
}
