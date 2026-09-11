use crate::proc;
use anyhow::{Context, Result};
use std::io::{self, BufRead, Write};
use std::process::Command;

/// The Keychain "account" every isolator-managed secret is stored under.
/// The service name (one per project+name pair) is what actually scopes
/// each item — see `service_name`.
const ACCOUNT: &str = "isolator";

fn service_name(project: &str, name: &str) -> String {
    format!("isolator-{project}-{name}")
}

/// Read a secret from the macOS Keychain, if present. Returns `Ok(None)`
/// both when the item genuinely doesn't exist and when `security` isn't
/// available at all (e.g. running this on a non-macOS host) — either way,
/// the caller falls back to "not resolvable from Keychain," never errors
/// the whole command out over an optional convenience feature.
pub fn get(project: &str, name: &str) -> Result<Option<String>> {
    let service = service_name(project, name);
    let result = proc::run_capture(
        "security",
        &["find-generic-password", "-a", ACCOUNT, "-s", &service, "-w"],
    );
    let (status, out) = match result {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    if !status.success() {
        return Ok(None);
    }
    let value = out.trim_end_matches('\n').to_string();
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

/// Interactively prompt for a secret's value and store it in the
/// Keychain, overwriting any existing value for this project+name.
///
/// The value is read from stdin with terminal echo disabled where
/// possible (via `stty -echo`/`stty echo` around the read — best-effort:
/// if stdin isn't a real TTY, e.g. in a script, this silently degrades to
/// a plain, visible read rather than failing). It is then passed to
/// `security add-generic-password -w <value>` as a literal argument —
/// `security`'s own `-h` output calls this "insecure" because the value
/// is briefly visible in that one subprocess's argv (e.g. to `ps` on this
/// same host, for the moment it runs). There's no better non-interactive
/// API in the stock `security` CLI; this is the same tradeoff every tool
/// scripting Keychain writes accepts.
pub fn set(project: &str, name: &str) -> Result<()> {
    let service = service_name(project, name);
    print!("Value for {name} (project '{project}'): ");
    io::stdout().flush().ok();
    let value = read_hidden_line()?;
    let value = value.trim();
    if value.is_empty() {
        anyhow::bail!("empty value — not stored");
    }

    let status = proc::run_capture(
        "security",
        &[
            "add-generic-password",
            "-a",
            ACCOUNT,
            "-s",
            &service,
            "-w",
            value,
            "-U",
        ],
    )
    .context("running `security add-generic-password`")?
    .0;
    proc::require_success("security add-generic-password", status)?;
    println!("stored {name} for project '{project}' in the macOS Keychain.");
    Ok(())
}

/// Remove a secret from the Keychain. Not an error if it wasn't there.
pub fn unset(project: &str, name: &str) -> Result<()> {
    let service = service_name(project, name);
    let _ = proc::run_capture(
        "security",
        &["delete-generic-password", "-a", ACCOUNT, "-s", &service],
    );
    println!("removed {name} for project '{project}' from the macOS Keychain (if it was there).");
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub enum Source {
    Env,
    Keychain,
    Missing,
}

/// Where each of a project's declared secrets would be resolved from
/// right now, in the same priority order `resolve_into_env` uses: this
/// process's own environment first, then the Keychain.
pub fn status(project: &str, secret_names: &[String]) -> Vec<(String, Source)> {
    secret_names
        .iter()
        .map(|name| {
            let source = if std::env::var(name).is_ok() {
                Source::Env
            } else if matches!(get(project, name), Ok(Some(_))) {
                Source::Keychain
            } else {
                Source::Missing
            };
            (name.clone(), source)
        })
        .collect()
}

/// Resolve every declared secret that isn't already in this process's own
/// environment from the Keychain, and set it there. Two independent
/// things depend on this: `docker compose` (which reads `${VAR}`
/// substitutions from its parent process's environment, in `compose_up`)
/// and `audit::redact` (which scrubs a secret's value out of logged
/// command argv, in every command that touches the audit chain — `run`,
/// `shell`, `new`). Call this near the top of any command that does
/// either. Never errors on a missing secret: an unresolvable name is left
/// unset, same as if the operator forgot to export it.
pub fn resolve_into_env(project: &str, secret_names: &[String]) {
    for name in secret_names {
        if std::env::var(name).is_ok() {
            continue;
        }
        if let Ok(Some(value)) = get(project, name) {
            std::env::set_var(name, value);
        }
    }
}

fn read_hidden_line() -> Result<String> {
    use std::process::Stdio;
    // stdout/stderr suppressed: when stdin isn't a real TTY (a script, a
    // pipe), `stty` fails with a message on stderr that isn't useful here
    // — the fallback (plain, visible read) is silent and expected.
    let echo_disabled = Command::new("stty")
        .arg("-echo")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;

    if echo_disabled {
        let _ = Command::new("stty")
            .arg("echo")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        println!();
    }

    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_name_is_scoped_by_project_and_secret_name() {
        assert_eq!(service_name("my-app", "GITHUB_TOKEN"), "isolator-my-app-GITHUB_TOKEN");
        assert_ne!(
            service_name("project-a", "API_KEY"),
            service_name("project-b", "API_KEY"),
            "two projects must never collide on the same Keychain service name"
        );
    }

    #[test]
    fn status_reports_env_before_checking_keychain() {
        std::env::set_var("ISOLATOR_TEST_STATUS_SECRET", "some-value");
        let result = status("some-project", &["ISOLATOR_TEST_STATUS_SECRET".to_string()]);
        assert_eq!(result, vec![("ISOLATOR_TEST_STATUS_SECRET".to_string(), Source::Env)]);
        std::env::remove_var("ISOLATOR_TEST_STATUS_SECRET");
    }

    #[test]
    fn status_reports_missing_when_neither_env_nor_keychain_has_it() {
        std::env::remove_var("ISOLATOR_TEST_STATUS_SECRET_UNSET");
        let result = status(
            "isolator-secrets-test-project-that-does-not-exist",
            &["ISOLATOR_TEST_STATUS_SECRET_UNSET".to_string()],
        );
        assert_eq!(
            result,
            vec![("ISOLATOR_TEST_STATUS_SECRET_UNSET".to_string(), Source::Missing)]
        );
    }
}
