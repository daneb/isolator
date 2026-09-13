use crate::{manifest::Manifest, paths, proc};
use anyhow::Result;

pub fn run() -> Result<()> {
    let names = paths::all_project_names()?;
    if names.is_empty() {
        println!("no projects yet — try `isolator new <name>`");
        return Ok(());
    }

    println!("{:<20} {:<24} {:<10} GITHUB", "NAME", "IMAGE", "STATUS");
    for name in names {
        let manifest_path = paths::manifest_path(&name)?;
        if !manifest_path.exists() {
            continue;
        }
        let m = Manifest::load(&manifest_path)?;
        let (_status, out) = proc::run_capture(
            "docker",
            &[
                "ps",
                "--filter",
                &format!("name={}", m.sandbox_container()),
                "--format",
                "{{.Status}}",
            ],
        )?;
        let running = if out.trim().is_empty() { "down" } else { "up" };
        println!(
            "{:<20} {:<24} {:<10} {}",
            m.name,
            m.image,
            running,
            m.github_repo.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}
