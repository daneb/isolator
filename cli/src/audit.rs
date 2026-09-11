use crate::paths;
use anyhow::{Context, Result};
use serde_json::json;
use std::io::Write;

/// Append one line to the project's host-only exec audit log. This is the
/// only writer of that file — no container ever has this path mounted, so
/// nothing running inside a sandbox can edit or delete an entry.
pub fn log_exec(project: &str, argv: &[String], exit_code: Option<i32>) -> Result<()> {
    paths::ensure_project_dirs(project)?;
    let path = paths::exec_log_path(project)?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening audit log {}", path.display()))?;

    let entry = json!({
        "ts": chrono::Utc::now().to_rfc3339(),
        "project": project,
        "argv": argv,
        "exit_code": exit_code,
    });
    writeln!(f, "{entry}")?;
    Ok(())
}
