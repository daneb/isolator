use anyhow::Result;

pub fn run(name: &str) -> Result<()> {
    super::compose_down(name)?;
    println!("'{name}' is down.");
    Ok(())
}
