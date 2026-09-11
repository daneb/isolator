use anyhow::{Context, Result};
use std::path::PathBuf;

pub fn isolator_home() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".isolator"))
}

pub fn project_dir(name: &str) -> Result<PathBuf> {
    Ok(isolator_home()?.join("projects").join(name))
}

pub fn manifest_path(name: &str) -> Result<PathBuf> {
    Ok(project_dir(name)?.join("isolator.yaml"))
}

pub fn compose_path(name: &str) -> Result<PathBuf> {
    Ok(project_dir(name)?.join("compose.yml"))
}

pub fn audit_dir(name: &str) -> Result<PathBuf> {
    Ok(project_dir(name)?.join("audit"))
}

/// The single hash-chained audit trail for a project — exec commands,
/// folded-in egress verdicts, and tripwire hits all append here. See
/// audit.rs.
pub fn chain_log_path(name: &str) -> Result<PathBuf> {
    Ok(audit_dir(name)?.join("chain.jsonl"))
}

/// How many raw lines of the egress gateway's access log have already
/// been folded into chain.jsonl — lets `isolator audit` be idempotent.
pub fn egress_offset_path(name: &str) -> Result<PathBuf> {
    Ok(audit_dir(name)?.join(".egress-offset"))
}

pub fn ensure_project_dirs(name: &str) -> Result<()> {
    std::fs::create_dir_all(audit_dir(name)?)?;
    Ok(())
}

pub fn all_project_names() -> Result<Vec<String>> {
    let projects_dir = isolator_home()?.join("projects");
    if !projects_dir.exists() {
        return Ok(vec![]);
    }
    let mut names = vec![];
    for entry in std::fs::read_dir(projects_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}
