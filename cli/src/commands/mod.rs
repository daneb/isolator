pub mod audit_cmd;
pub mod down;
pub mod new_cmd;
pub mod run_cmd;
pub mod selftest;
pub mod shell;
pub mod status;
pub mod up;

use crate::{compose, manifest::Manifest, paths};
use anyhow::{Context, Result};

/// Re-render compose.yml from the current manifest and bring the pair up.
/// Called by both `new` and `up` so a hand-edited isolator.yaml always
/// takes effect on the next start, not just at creation time.
pub fn compose_up(name: &str) -> Result<()> {
    let m = Manifest::load(&paths::manifest_path(name)?)?;
    let rendered = compose::render(&m);
    let compose_path = paths::compose_path(name)?;
    std::fs::write(&compose_path, rendered)
        .with_context(|| format!("writing {}", compose_path.display()))?;

    let compose_path_str = compose_path.to_string_lossy().to_string();
    let status = crate::proc::run_inherit(
        "docker",
        &[
            "compose",
            "-p",
            name,
            "-f",
            &compose_path_str,
            "up",
            "-d",
        ],
    )?;
    crate::proc::require_success("docker compose up", status)?;
    Ok(())
}

pub fn compose_down(name: &str) -> Result<()> {
    let compose_path = paths::compose_path(name)?;
    if !compose_path.exists() {
        anyhow::bail!("no compose.yml for project '{name}' — has `isolator new` been run?");
    }
    let compose_path_str = compose_path.to_string_lossy().to_string();
    let status = crate::proc::run_inherit(
        "docker",
        &[
            "compose",
            "-p",
            name,
            "-f",
            &compose_path_str,
            "down",
        ],
    )?;
    crate::proc::require_success("docker compose down", status)?;
    Ok(())
}
