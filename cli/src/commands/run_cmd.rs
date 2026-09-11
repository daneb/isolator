use crate::{audit, manifest::Manifest, paths, proc};
use anyhow::Result;

pub fn run(name: &str, cmd: &[String]) -> Result<()> {
    if cmd.is_empty() {
        anyhow::bail!("usage: isolator run <project> -- <command...>");
    }
    let m = Manifest::load(&paths::manifest_path(name)?)?;
    let container = m.sandbox_container();

    let mut args: Vec<&str> = vec!["exec", &container];
    args.extend(cmd.iter().map(|s| s.as_str()));

    let status = proc::run_inherit("docker", &args)?;
    audit::log_exec(name, cmd, status.code())?;
    proc::require_success(&format!("`{}` in {container}", cmd.join(" ")), status)?;
    Ok(())
}
