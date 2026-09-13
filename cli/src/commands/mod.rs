pub mod audit_cmd;
pub mod down;
pub mod import_cmd;
pub mod new_cmd;
pub mod run_cmd;
pub mod secrets_status;
pub mod selftest;
pub mod shell;
pub mod status;
pub mod up;

use crate::{compose, manifest::Manifest, paths};
use anyhow::{Context, Result};
use std::io::{self, Write};

/// Interactively confirm and create a private GitHub repo named `name`,
/// returning its "owner/repo" slug. `Ok(None)` means the operator
/// declined the confirmation prompt — callers should treat that as "stop
/// here, don't scaffold anything," not as an error. Shared by `new` and
/// `import` so the confirm-then-create-then-report flow can't drift
/// between the two.
pub fn create_github_repo_interactive(name: &str) -> Result<Option<String>> {
    print!(
        "About to run `gh repo create {name} --private` on your GitHub account. Continue? [y/N] "
    );
    io::stdout().flush().ok();
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if !answer.trim().eq_ignore_ascii_case("y") {
        return Ok(None);
    }

    let (status, whoami) = crate::proc::run_capture("gh", &["api", "user", "-q", ".login"])?;
    crate::proc::require_success("gh api user", status)?;
    let owner = whoami.trim();
    let repo_slug = format!("{owner}/{name}");

    let status = crate::proc::run_inherit("gh", &["repo", "create", &repo_slug, "--private"])?;
    crate::proc::require_success("gh repo create", status)?;
    Ok(Some(repo_slug))
}

/// Re-render compose.yml from the current manifest and bring the pair up.
/// Called by both `new` and `up` so a hand-edited moor.yaml always
/// takes effect on the next start, not just at creation time.
pub fn compose_up(name: &str) -> Result<()> {
    let m = Manifest::load(&paths::manifest_path(name)?)?;
    // Anything not already in this process's own environment gets a
    // chance to resolve from the Keychain before we hand off to `docker
    // compose`, which reads ${VAR} substitutions from its parent
    // process's environment — see secrets.rs.
    crate::secrets::resolve_into_env(name, &m.secrets);
    let rendered = compose::render(&m);
    let compose_path = paths::compose_path(name)?;
    std::fs::write(&compose_path, rendered)
        .with_context(|| format!("writing {}", compose_path.display()))?;

    let compose_path_str = compose_path.to_string_lossy().to_string();
    let status = crate::proc::run_inherit(
        "docker",
        &["compose", "-p", name, "-f", &compose_path_str, "up", "-d"],
    )?;
    crate::proc::require_success("docker compose up", status)?;
    Ok(())
}

pub fn compose_down(name: &str) -> Result<()> {
    let compose_path = paths::compose_path(name)?;
    if !compose_path.exists() {
        anyhow::bail!("no compose.yml for project '{name}' — has `moor new` been run?");
    }
    let compose_path_str = compose_path.to_string_lossy().to_string();
    let status = crate::proc::run_inherit(
        "docker",
        &["compose", "-p", name, "-f", &compose_path_str, "down"],
    )?;
    crate::proc::require_success("docker compose down", status)?;
    Ok(())
}
