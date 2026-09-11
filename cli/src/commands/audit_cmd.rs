use crate::paths;
use anyhow::Result;

/// v1: show the host-side audit trail for a project. The full pipeline
/// (folding in the egress proxy's log and keel's own evidence bundles) is
/// Phase 6 — this is deliberately just the exec log for now, not a stub
/// that pretends to be more than it is.
pub fn run(name: &str) -> Result<()> {
    let log = paths::exec_log_path(name)?;
    if !log.exists() {
        println!("no audit entries yet for '{name}' ({})", log.display());
        return Ok(());
    }
    println!("== exec log: {} ==", log.display());
    let text = std::fs::read_to_string(&log)?;
    for line in text.lines() {
        println!("{line}");
    }
    println!(
        "\nNote: this is the exec log only. Egress proxy logs and keel's own\n\
         evidence bundles (.keel/store/evidence/) are not yet folded in —\n\
         see Phase 6 in the isolator plan. Egress logs for this project can\n\
         be read directly with: docker logs {name}-egress"
    );
    Ok(())
}
