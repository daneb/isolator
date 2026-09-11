use crate::{audit, manifest::Manifest, paths, proc};
use anyhow::Result;

/// True if the invoked command looks like a `git push` — the one channel
/// through which code actually leaves the sandbox for real (as opposed
/// to routine egress like `git fetch`/`clone`/package installs). Tagged
/// distinctly in the audit chain so it stands out from routine execs.
/// Best-effort by nature: it only sees commands run via `isolator run`,
/// not ones typed inside an interactive `isolator shell` session — see
/// docs/THREAT-MODEL.md for why a git hook baked into the image isn't a
/// stronger alternative (it runs inside the untrusted sandbox and the
/// agent can simply reconfigure or bypass it).
fn looks_like_git_push(cmd: &[String]) -> bool {
    cmd.first().map(|s| s == "git").unwrap_or(false) && cmd.iter().any(|a| a == "push")
}

pub fn run(name: &str, cmd: &[String]) -> Result<()> {
    if cmd.is_empty() {
        anyhow::bail!("usage: isolator run <project> -- <command...>");
    }
    let m = Manifest::load(&paths::manifest_path(name)?)?;
    // So audit::redact below can scrub a secret's value even when it was
    // only ever set via Keychain, never exported into this shell.
    crate::secrets::resolve_into_env(name, &m.secrets);
    let container = m.sandbox_container();

    let mut args: Vec<&str> = vec!["exec", &container];
    args.extend(cmd.iter().map(|s| s.as_str()));

    let status = proc::run_inherit("docker", &args)?;
    let kind = if looks_like_git_push(cmd) { "git-push" } else { "exec" };
    audit::log_exec(name, &m, kind, cmd, status.code())?;
    proc::require_success(&format!("`{}` in {container}", cmd.join(" ")), status)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_plain_git_push() {
        assert!(looks_like_git_push(&["git".into(), "push".into()]));
        assert!(looks_like_git_push(&[
            "git".into(),
            "push".into(),
            "origin".into(),
            "main".into()
        ]));
    }

    #[test]
    fn does_not_flag_other_git_commands() {
        assert!(!looks_like_git_push(&["git".into(), "clone".into(), "x".into()]));
        assert!(!looks_like_git_push(&["git".into(), "fetch".into()]));
        assert!(!looks_like_git_push(&["git".into(), "status".into()]));
    }

    #[test]
    fn does_not_flag_non_git_commands() {
        assert!(!looks_like_git_push(&["keel".into(), "run".into()]));
        assert!(!looks_like_git_push(&["npm".into(), "run".into(), "push".into()]));
    }

    #[test]
    fn empty_command_is_not_a_push() {
        assert!(!looks_like_git_push(&[]));
    }
}
