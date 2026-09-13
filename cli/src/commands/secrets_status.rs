use crate::{manifest::Manifest, paths, secrets};
use anyhow::Result;

pub fn run(project: &str) -> Result<()> {
    let m = Manifest::load(&paths::manifest_path(project)?)?;
    if m.secrets.is_empty() {
        println!("'{project}' declares no secrets (isolator.yaml's secrets: list is empty).");
        return Ok(());
    }

    println!("{:<24} SOURCE", "SECRET");
    for (name, source) in secrets::status(project, &m.secrets) {
        let label = match source {
            secrets::Source::Env => "your shell's environment",
            secrets::Source::Keychain => "macOS Keychain",
            secrets::Source::Missing => "NOT SET — export it, or `isolator secrets set`",
        };
        println!("{name:<24} {label}");
    }
    Ok(())
}
