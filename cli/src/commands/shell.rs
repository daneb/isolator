use crate::{audit, manifest::Manifest, paths, proc};
use anyhow::Result;

pub fn run(name: &str) -> Result<()> {
    let m = Manifest::load(&paths::manifest_path(name)?)?;
    crate::secrets::resolve_into_env(name, &m.secrets);
    let container = m.sandbox_container();
    println!("==> attaching to {container} (exit the shell to detach)");
    let status = proc::run_inherit(
        "docker",
        &["exec", "-it", &container, "/bin/bash"],
    )?;
    audit::log_exec(name, &m, "exec", &["shell".into()], status.code())?;
    Ok(())
}
