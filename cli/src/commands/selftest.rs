use crate::{audit, canary::CANARY_DOMAIN, manifest::Manifest, paths, proc};
use anyhow::{Context, Result};
use serde_json::Value;

struct Check {
    label: &'static str,
    pass: bool,
    detail: String,
}

/// Run one probe inside the sandbox and decide pass/fail from whether it
/// was blocked the way it should have been. These need a live container
/// (unlike `evaluate()` above) so they aren't unit-tested directly — the
/// e2e script in tests/e2e.sh exercises them against a real sandbox.
/// Run one probe and turn its (possibly failed-to-even-launch) result
/// into a Check. `pass_when` decides pass/fail from (exit_success, stdout)
/// — always evaluated as `false` if the probe itself couldn't be run at
/// all, since an unrunnable probe proves nothing.
fn probe(
    label: &'static str,
    result: Result<(std::process::ExitStatus, String)>,
    detail_prefix: &str,
    pass_when: impl Fn(bool, &str) -> bool,
) -> Check {
    match result {
        Ok((status, out)) => Check {
            label,
            pass: pass_when(status.success(), out.trim()),
            detail: format!(
                "{detail_prefix}: exit_success={} out={:?}",
                status.success(),
                out.trim()
            ),
        },
        Err(e) => Check {
            label,
            pass: false,
            detail: format!("{detail_prefix}: could not run probe: {e}"),
        },
    }
}

fn run_breakout_battery(container: &str) -> Vec<Check> {
    vec![
        probe(
            "canary domain is unreachable",
            proc::run_capture(
                "docker",
                &[
                    "exec",
                    container,
                    "curl",
                    "-sS",
                    "-o",
                    "/dev/null",
                    "--max-time",
                    "5",
                    &format!("https://{CANARY_DOMAIN}"),
                ],
            ),
            &format!("curl https://{CANARY_DOMAIN}"),
            |success, _| !success,
        ),
        probe(
            "root filesystem rejects writes",
            proc::run_capture(
                "docker",
                &[
                    "exec",
                    container,
                    "sh",
                    "-c",
                    "touch /isolator-write-test 2>&1",
                ],
            ),
            "touch /isolator-write-test",
            |success, _| !success,
        ),
        probe(
            "docker.sock is not present",
            proc::run_capture(
                "docker",
                &["exec", container, "test", "-S", "/var/run/docker.sock"],
            ),
            "test -S /var/run/docker.sock",
            |success, _| !success,
        ),
    ]
}

/// Pure evaluation of a `docker inspect <container>` result (the `[0]`
/// element of the parsed JSON array) against every row of the hardening
/// table in policies/README.md. Kept separate from `run()` so it can be
/// unit-tested without Docker — see the `tests` module below.
fn evaluate(info: &Value) -> Vec<Check> {
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

    // Absence of the Mounts field entirely is treated as unknown, not
    // "no mounts" — a malformed/truncated `docker inspect` result must
    // not be able to pass this check by omission.
    let mounts_present = info.get("Mounts").map(|v| !v.is_null()).unwrap_or(false);
    let mounts = info["Mounts"].as_array().cloned().unwrap_or_default();
    let bind_mounts: Vec<String> = mounts
        .iter()
        .filter(|mnt| mnt["Type"].as_str() == Some("bind"))
        .map(|mnt| mnt["Source"].as_str().unwrap_or("?").to_string())
        .collect();
    checks.push(Check {
        label: "no host bind mounts",
        pass: mounts_present && bind_mounts.is_empty(),
        detail: format!("bind mounts={bind_mounts:?} (Mounts field present={mounts_present})"),
    });

    let user = info["Config"]["User"].as_str().unwrap_or("");
    let non_root = !user.is_empty() && user != "0" && user != "root";
    checks.push(Check {
        label: "non-root user",
        pass: non_root,
        detail: format!("User='{user}'"),
    });

    // Same fail-safe reasoning as the mounts check above: a missing
    // NetworkSettings.Networks object must not read as "no networks
    // attached, therefore not on the default bridge".
    let networks_present = info
        .get("NetworkSettings")
        .and_then(|ns| ns.get("Networks"))
        .map(|v| !v.is_null())
        .unwrap_or(false);
    let networks = info["NetworkSettings"]["Networks"]
        .as_object()
        .map(|o| o.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let on_default_bridge = networks.iter().any(|n| n == "bridge");
    checks.push(Check {
        label: "not on the default bridge network",
        pass: networks_present && !on_default_bridge,
        detail: format!("networks={networks:?} (Networks field present={networks_present})"),
    });

    let pids_limit = info["HostConfig"]["PidsLimit"].as_i64().unwrap_or(0);
    checks.push(Check {
        label: "pids limit set",
        pass: pids_limit > 0,
        detail: format!("PidsLimit={pids_limit}"),
    });

    checks
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
        anyhow::bail!(
            "`docker inspect {container}` failed — is the project up? (`isolator up {name}`)"
        );
    }
    let parsed: Vec<Value> = serde_json::from_str(&out).context("parsing docker inspect output")?;
    let info = parsed
        .first()
        .context("docker inspect returned no containers")?;

    let mut checks = evaluate(info);
    println!("-- static hardening checks (docker inspect) --");
    for c in &checks {
        let mark = if c.pass { "PASS" } else { "FAIL" };
        println!("[{mark}] {} ({})", c.label, c.detail);
    }

    println!("\n-- active breakout battery (live probes) --");
    let battery = run_breakout_battery(&container);
    for c in &battery {
        let mark = if c.pass { "PASS" } else { "FAIL" };
        println!("[{mark}] {} ({})", c.label, c.detail);
    }
    audit::log_exec(
        name,
        &m,
        "tripwire-check",
        &["selftest".into(), "breakout-battery".into()],
        None,
    )
    .ok();
    let folded = audit::fold_egress_log(name, &m).unwrap_or(0);
    if folded > 0 {
        println!("\n(folded {folded} new egress-log entries into the audit chain — check `isolator audit {name}` for any TRIPWIRE lines)");
    }

    checks.extend(battery.into_iter().map(|c| Check {
        label: c.label,
        pass: c.pass,
        detail: c.detail,
    }));
    let all_pass = checks.iter().all(|c| c.pass);

    if all_pass {
        println!("\nselftest: all checks passed for '{name}'.");
        Ok(())
    } else {
        anyhow::bail!(
            "selftest: one or more hardening checks failed for '{name}' — see FAIL lines above"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A `docker inspect` result matching everything the compose template
    /// asks for — this should evaluate to all-pass.
    fn hardened_container() -> Value {
        json!({
            "HostConfig": {
                "ReadonlyRootfs": true,
                "CapDrop": ["ALL"],
                "SecurityOpt": ["no-new-privileges:true"],
                "Privileged": false,
                "PidsLimit": 512
            },
            "Config": { "User": "agent" },
            "Mounts": [
                {"Type": "volume", "Source": "myproj-workspace"},
                {"Type": "tmpfs", "Source": ""}
            ],
            "NetworkSettings": {
                "Networks": { "myproj-internal": {} }
            }
        })
    }

    #[test]
    fn hardened_container_passes_every_check() {
        let checks = evaluate(&hardened_container());
        assert_eq!(checks.len(), 8, "expected 8 hardening checks");
        for c in &checks {
            assert!(
                c.pass,
                "expected '{}' to pass, detail={}",
                c.label, c.detail
            );
        }
    }

    #[test]
    fn writable_rootfs_fails() {
        let mut v = hardened_container();
        v["HostConfig"]["ReadonlyRootfs"] = json!(false);
        let checks = evaluate(&v);
        let c = checks
            .iter()
            .find(|c| c.label == "read-only root filesystem")
            .unwrap();
        assert!(!c.pass);
    }

    #[test]
    fn missing_cap_drop_all_fails() {
        let mut v = hardened_container();
        v["HostConfig"]["CapDrop"] = json!(["NET_ADMIN"]);
        let checks = evaluate(&v);
        let c = checks
            .iter()
            .find(|c| c.label == "all capabilities dropped")
            .unwrap();
        assert!(!c.pass);
    }

    #[test]
    fn missing_no_new_privileges_fails() {
        let mut v = hardened_container();
        v["HostConfig"]["SecurityOpt"] = json!([]);
        let checks = evaluate(&v);
        let c = checks
            .iter()
            .find(|c| c.label == "no-new-privileges set")
            .unwrap();
        assert!(!c.pass);
    }

    #[test]
    fn privileged_container_fails() {
        let mut v = hardened_container();
        v["HostConfig"]["Privileged"] = json!(true);
        let checks = evaluate(&v);
        let c = checks.iter().find(|c| c.label == "not privileged").unwrap();
        assert!(!c.pass);
    }

    #[test]
    fn bind_mount_fails() {
        let mut v = hardened_container();
        v["Mounts"] = json!([{"Type": "bind", "Source": "/Users/danebalia"}]);
        let checks = evaluate(&v);
        let c = checks
            .iter()
            .find(|c| c.label == "no host bind mounts")
            .unwrap();
        assert!(!c.pass);
        assert!(c.detail.contains("/Users/danebalia"));
    }

    #[test]
    fn root_user_fails() {
        for root_spelling in ["root", "0", ""] {
            let mut v = hardened_container();
            v["Config"]["User"] = json!(root_spelling);
            let checks = evaluate(&v);
            let c = checks.iter().find(|c| c.label == "non-root user").unwrap();
            assert!(!c.pass, "'{root_spelling}' should not count as non-root");
        }
    }

    #[test]
    fn default_bridge_network_fails() {
        let mut v = hardened_container();
        v["NetworkSettings"]["Networks"] = json!({"bridge": {}});
        let checks = evaluate(&v);
        let c = checks
            .iter()
            .find(|c| c.label == "not on the default bridge network")
            .unwrap();
        assert!(!c.pass);
    }

    #[test]
    fn zero_pids_limit_fails() {
        let mut v = hardened_container();
        v["HostConfig"]["PidsLimit"] = json!(0);
        let checks = evaluate(&v);
        let c = checks.iter().find(|c| c.label == "pids limit set").unwrap();
        assert!(!c.pass);
    }

    #[test]
    fn missing_fields_default_to_failing_safe() {
        // An empty object should never accidentally evaluate as hardened —
        // every check's default must lean toward FAIL, not PASS.
        let checks = evaluate(&json!({}));
        assert!(
            checks.iter().all(|c| !c.pass),
            "empty inspect output must fail every check"
        );
    }
}
