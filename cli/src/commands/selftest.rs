use crate::{manifest::Manifest, paths, proc};
use anyhow::{Context, Result};
use serde_json::Value;

struct Check {
    label: &'static str,
    pass: bool,
    detail: String,
}

/// Inspect the live sandbox container and assert every row of the
/// hardening table in policies/README.md actually holds. This is the
/// guardrail that catches a future template change silently weakening
/// isolation — see Phase 7 in the isolator plan for the full breakout
/// battery this will grow into.
pub fn run(name: &str) -> Result<()> {
    let m = Manifest::load(&paths::manifest_path(name)?)?;
    let container = m.sandbox_container();

    let (status, out) = proc::run_capture("docker", &["inspect", &container])?;
    if !status.success() {
        anyhow::bail!("`docker inspect {container}` failed — is the project up? (`isolator up {name}`)");
    }
    let parsed: Vec<Value> = serde_json::from_str(&out).context("parsing docker inspect output")?;
    let info = parsed
        .first()
        .context("docker inspect returned no containers")?;

    let mut checks = vec![];

    let readonly = info["HostConfig"]["ReadonlyRootfs"]
        .as_bool()
        .unwrap_or(false);
    checks.push(Check {
        label: "read-only root filesystem",
        pass: readonly,
        detail: format!("ReadonlyRootfs={readonly}"),
    });

    let cap_drop = info["HostConfig"]["CapDrop"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();
    let drops_all = cap_drop.iter().any(|c| c.eq_ignore_ascii_case("ALL"));
    checks.push(Check {
        label: "all capabilities dropped",
        pass: drops_all,
        detail: format!("CapDrop={cap_drop:?}"),
    });

    let sec_opt = info["HostConfig"]["SecurityOpt"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();
    let no_new_priv = sec_opt.iter().any(|s| s.starts_with("no-new-privileges"));
    checks.push(Check {
        label: "no-new-privileges set",
        pass: no_new_priv,
        detail: format!("SecurityOpt={sec_opt:?}"),
    });

    let privileged = info["HostConfig"]["Privileged"].as_bool().unwrap_or(true);
    checks.push(Check {
        label: "not privileged",
        pass: !privileged,
        detail: format!("Privileged={privileged}"),
    });

    let mounts = info["Mounts"].as_array().cloned().unwrap_or_default();
    let bind_mounts: Vec<String> = mounts
        .iter()
        .filter(|mnt| mnt["Type"].as_str() == Some("bind"))
        .map(|mnt| mnt["Source"].as_str().unwrap_or("?").to_string())
        .collect();
    checks.push(Check {
        label: "no host bind mounts",
        pass: bind_mounts.is_empty(),
        detail: format!("bind mounts={bind_mounts:?}"),
    });

    let user = info["Config"]["User"].as_str().unwrap_or("");
    let non_root = !user.is_empty() && user != "0" && user != "root";
    checks.push(Check {
        label: "non-root user",
        pass: non_root,
        detail: format!("User='{user}'"),
    });

    let networks = info["NetworkSettings"]["Networks"]
        .as_object()
        .map(|o| o.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let on_default_bridge = networks.iter().any(|n| n == "bridge");
    checks.push(Check {
        label: "not on the default bridge network",
        pass: !on_default_bridge,
        detail: format!("networks={networks:?}"),
    });

    let pids_limit = info["HostConfig"]["PidsLimit"].as_i64().unwrap_or(0);
    checks.push(Check {
        label: "pids limit set",
        pass: pids_limit > 0,
        detail: format!("PidsLimit={pids_limit}"),
    });

    let mut all_pass = true;
    for c in &checks {
        let mark = if c.pass { "PASS" } else { "FAIL" };
        if !c.pass {
            all_pass = false;
        }
        println!("[{mark}] {} ({})", c.label, c.detail);
    }

    if all_pass {
        println!("\nselftest: all checks passed for '{name}'.");
        Ok(())
    } else {
        anyhow::bail!("selftest: one or more hardening checks failed for '{name}' — see FAIL lines above");
    }
}
