use anyhow::{Context, Result};
use std::process::{Command, ExitStatus, Stdio};

/// Run a command with stdio inherited (the operator sees output live,
/// e.g. `docker compose up`, `docker exec -it ... bash`). Returns the exit
/// status; callers decide whether a non-zero code is fatal.
pub fn run_inherit(program: &str, args: &[&str]) -> Result<ExitStatus> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("spawning `{program} {}`", args.join(" ")))
}

/// Run a command and capture stdout as a String (used for `docker inspect`,
/// `docker ps`, etc. where we need to parse the result).
pub fn run_capture(program: &str, args: &[&str]) -> Result<(ExitStatus, String)> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("spawning `{program} {}`", args.join(" ")))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok((output.status, stdout))
}


pub fn require_success(what: &str, status: ExitStatus) -> Result<()> {
    if !status.success() {
        anyhow::bail!("{what} failed ({status})");
    }
    Ok(())
}
