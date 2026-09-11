use crate::{manifest::Manifest, paths};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::Path;

const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// One entry in a project's audit hash chain. `hash` commits to every
/// other field including `prev_hash` — the hash of the entry before it —
/// so the file is a Merkle-style chain: editing, deleting, or reordering
/// any entry breaks the hash of every entry after it. Nothing inside a
/// sandbox has this path mounted, so the only way to produce a line that
/// verifies is to have generated it honestly at the time; see
/// `verify_chain` and `docs/THREAT-MODEL.md`'s "audit tampering" section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainEntry {
    pub seq: u64,
    pub ts: String,
    pub kind: String,
    pub data: Value,
    pub prev_hash: String,
    pub hash: String,
}

fn compute_hash(prev_hash: &str, seq: u64, ts: &str, kind: &str, data: &Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update(seq.to_le_bytes());
    hasher.update(ts.as_bytes());
    hasher.update(kind.as_bytes());
    hasher.update(data.to_string().as_bytes());
    hex::encode(hasher.finalize())
}

fn last_entry(path: &Path) -> Result<Option<ChainEntry>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    match text.lines().rev().find(|l| !l.trim().is_empty()) {
        Some(line) => Ok(Some(
            serde_json::from_str(line).with_context(|| format!("parsing last line of {}", path.display()))?,
        )),
        None => Ok(None),
    }
}

/// Append one entry to a project's hash-chained audit log. This is the
/// only writer of that file — no container ever has this path mounted.
pub fn append_chained(path: &Path, kind: &str, data: Value) -> Result<ChainEntry> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let (seq, prev_hash) = match last_entry(path)? {
        Some(e) => (e.seq + 1, e.hash),
        None => (0, GENESIS_HASH.to_string()),
    };
    let ts = chrono::Utc::now().to_rfc3339();
    let hash = compute_hash(&prev_hash, seq, &ts, kind, &data);
    let entry = ChainEntry {
        seq,
        ts,
        kind: kind.to_string(),
        data,
        prev_hash,
        hash,
    };

    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("opening audit log {}", path.display()))?;
    writeln!(f, "{}", serde_json::to_string(&entry)?)?;
    Ok(entry)
}

#[derive(Debug, PartialEq, Eq)]
pub enum VerifyOutcome {
    Ok { entries: usize },
    Tampered { at_seq: u64, reason: String },
}

/// Walk the whole chain and recompute every hash. Any edit, deletion,
/// reorder, or truncation-from-the-middle of the file will show up here
/// as a mismatch at the first affected entry.
pub fn verify_chain(path: &Path) -> Result<VerifyOutcome> {
    if !path.exists() {
        return Ok(VerifyOutcome::Ok { entries: 0 });
    }
    let text = std::fs::read_to_string(path)?;
    let mut prev_hash = GENESIS_HASH.to_string();
    let mut expected_seq: u64 = 0;
    let mut count = 0;

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let entry: ChainEntry = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(e) => {
                return Ok(VerifyOutcome::Tampered {
                    at_seq: expected_seq,
                    reason: format!("entry at seq {expected_seq} is not valid JSON: {e}"),
                })
            }
        };
        if entry.seq != expected_seq {
            return Ok(VerifyOutcome::Tampered {
                at_seq: entry.seq,
                reason: format!("expected seq {expected_seq}, found {} (an entry was deleted or reordered)", entry.seq),
            });
        }
        if entry.prev_hash != prev_hash {
            return Ok(VerifyOutcome::Tampered {
                at_seq: entry.seq,
                reason: "prev_hash does not match the preceding entry's hash".to_string(),
            });
        }
        let recomputed = compute_hash(&entry.prev_hash, entry.seq, &entry.ts, &entry.kind, &entry.data);
        if recomputed != entry.hash {
            return Ok(VerifyOutcome::Tampered {
                at_seq: entry.seq,
                reason: "hash does not match entry contents (the entry itself was edited)".to_string(),
            });
        }
        prev_hash = entry.hash;
        expected_seq += 1;
        count += 1;
    }

    Ok(VerifyOutcome::Ok { entries: count })
}

/// Best-effort redaction: replace any literal occurrence of a named
/// secret's *value* (as currently set in this process's own environment)
/// with a placeholder. This only catches secrets the isolator CLI itself
/// had access to at logging time — see docs/THREAT-MODEL.md and
/// proxy/README.md for what this control does and doesn't cover.
pub fn redact(text: &str, secret_names: &[String]) -> String {
    let mut out = text.to_string();
    for name in secret_names {
        if let Ok(val) = std::env::var(name) {
            if val.len() >= 4 {
                out = out.replace(&val, &format!("***REDACTED:{name}***"));
            }
        }
    }
    out
}

fn redact_argv(argv: &[String], secret_names: &[String]) -> Vec<String> {
    argv.iter().map(|a| redact(a, secret_names)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// A fresh, never-before-used path under the OS temp dir, unique per
    /// call (parallel test threads must never collide on the same file).
    fn temp_path(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("isolator-audit-test-{label}-{nanos}.jsonl"))
    }

    #[test]
    fn empty_or_missing_chain_verifies_ok_with_zero_entries() {
        let path = temp_path("missing");
        assert_eq!(verify_chain(&path).unwrap(), VerifyOutcome::Ok { entries: 0 });
    }

    #[test]
    fn a_freshly_appended_chain_verifies_ok() {
        let path = temp_path("fresh");
        append_chained(&path, "exec", json!({"argv": ["keel", "status"]})).unwrap();
        append_chained(&path, "exec", json!({"argv": ["keel", "run", "spec-1"]})).unwrap();
        append_chained(&path, "egress", json!({"domain": "github.com"})).unwrap();

        assert_eq!(verify_chain(&path).unwrap(), VerifyOutcome::Ok { entries: 3 });
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn seq_and_prev_hash_link_correctly() {
        let path = temp_path("linkage");
        let e0 = append_chained(&path, "exec", json!({"n": 0})).unwrap();
        let e1 = append_chained(&path, "exec", json!({"n": 1})).unwrap();
        assert_eq!(e0.seq, 0);
        assert_eq!(e0.prev_hash, GENESIS_HASH);
        assert_eq!(e1.seq, 1);
        assert_eq!(e1.prev_hash, e0.hash);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn editing_an_entrys_data_is_detected() {
        let path = temp_path("edit-data");
        append_chained(&path, "exec", json!({"argv": ["safe", "command"]})).unwrap();
        append_chained(&path, "exec", json!({"argv": ["another", "command"]})).unwrap();

        // Tamper: rewrite the first line's data field without recomputing
        // its hash — simulating someone hand-editing the file to hide
        // what a command actually was.
        let text = std::fs::read_to_string(&path).unwrap();
        let mut lines: Vec<String> = text.lines().map(String::from).collect();
        let mut entry: ChainEntry = serde_json::from_str(&lines[0]).unwrap();
        entry.data = json!({"argv": ["rm", "-rf", "/"]});
        lines[0] = serde_json::to_string(&entry).unwrap();
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();

        match verify_chain(&path).unwrap() {
            VerifyOutcome::Tampered { at_seq, .. } => assert_eq!(at_seq, 0),
            VerifyOutcome::Ok { .. } => panic!("tampering was not detected"),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn deleting_a_middle_entry_is_detected() {
        let path = temp_path("delete-middle");
        append_chained(&path, "exec", json!({"n": 0})).unwrap();
        append_chained(&path, "exec", json!({"n": 1})).unwrap();
        append_chained(&path, "exec", json!({"n": 2})).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        // Drop the middle entry (seq 1) — first and last are left intact.
        let tampered = format!("{}\n{}\n", lines[0], lines[2]);
        std::fs::write(&path, tampered).unwrap();

        match verify_chain(&path).unwrap() {
            VerifyOutcome::Tampered { .. } => {}
            VerifyOutcome::Ok { .. } => panic!("deletion was not detected"),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reordering_entries_is_detected() {
        let path = temp_path("reorder");
        append_chained(&path, "exec", json!({"n": 0})).unwrap();
        append_chained(&path, "exec", json!({"n": 1})).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        let swapped = format!("{}\n{}\n", lines[1], lines[0]);
        std::fs::write(&path, swapped).unwrap();

        match verify_chain(&path).unwrap() {
            VerifyOutcome::Tampered { .. } => {}
            VerifyOutcome::Ok { .. } => panic!("reordering was not detected"),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn appending_a_forged_entry_without_the_real_chain_state_is_detected() {
        let path = temp_path("forge-append");
        append_chained(&path, "exec", json!({"n": 0})).unwrap();

        // An attacker who doesn't control this process can't call
        // append_chained (that's the point) — simulate them hand-writing
        // a plausible-looking next line with an invented prev_hash/hash.
        let forged = ChainEntry {
            seq: 1,
            ts: chrono::Utc::now().to_rfc3339(),
            kind: "exec".to_string(),
            data: json!({"argv": ["totally", "legitimate"]}),
            prev_hash: "f".repeat(64),
            hash: "0".repeat(64),
        };
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        use std::io::Write;
        writeln!(f, "{}", serde_json::to_string(&forged).unwrap()).unwrap();

        match verify_chain(&path).unwrap() {
            VerifyOutcome::Tampered { at_seq, .. } => assert_eq!(at_seq, 1),
            VerifyOutcome::Ok { .. } => panic!("forged append was not detected"),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn truncating_to_a_prefix_still_verifies_ok() {
        // Losing the tail of the file (e.g. a crash mid-write) is not the
        // same as tampering with what's left — the surviving prefix must
        // still verify, since every entry in it is exactly as it was
        // originally written.
        let path = temp_path("truncate");
        append_chained(&path, "exec", json!({"n": 0})).unwrap();
        append_chained(&path, "exec", json!({"n": 1})).unwrap();
        append_chained(&path, "exec", json!({"n": 2})).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let first_line = text.lines().next().unwrap();
        std::fs::write(&path, format!("{first_line}\n")).unwrap();

        assert_eq!(verify_chain(&path).unwrap(), VerifyOutcome::Ok { entries: 1 });
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn redact_replaces_secret_env_values_but_leaves_everything_else() {
        std::env::set_var("ISOLATOR_TEST_SECRET_A", "sk-super-secret-value-123");
        let text = "curl -H 'Authorization: Bearer sk-super-secret-value-123' https://api.example.com";
        let redacted = redact(text, &["ISOLATOR_TEST_SECRET_A".to_string()]);
        assert!(!redacted.contains("sk-super-secret-value-123"));
        assert!(redacted.contains("***REDACTED:ISOLATOR_TEST_SECRET_A***"));
        assert!(redacted.contains("https://api.example.com"));
        std::env::remove_var("ISOLATOR_TEST_SECRET_A");
    }

    #[test]
    fn redact_ignores_secrets_not_set_in_env() {
        std::env::remove_var("ISOLATOR_TEST_SECRET_UNSET");
        let text = "keel run my-spec";
        let redacted = redact(text, &["ISOLATOR_TEST_SECRET_UNSET".to_string()]);
        assert_eq!(redacted, text);
    }

    #[test]
    fn redact_does_not_mangle_trivially_short_env_values() {
        // A short/empty env value (e.g. an accidentally-blank secret)
        // must not turn into a blanket find-and-replace of common
        // substrings across the whole log line.
        std::env::set_var("ISOLATOR_TEST_SHORT", "ok");
        let text = "echo ok, this token is ok";
        let redacted = redact(text, &["ISOLATOR_TEST_SHORT".to_string()]);
        assert_eq!(redacted, text, "short values must not trigger redaction");
        std::env::remove_var("ISOLATOR_TEST_SHORT");
    }

    #[test]
    fn redact_argv_redacts_each_element_independently() {
        std::env::set_var("ISOLATOR_TEST_SECRET_B", "ghp_abcdef1234567890");
        let argv = vec![
            "curl".to_string(),
            "-H".to_string(),
            "Authorization: token ghp_abcdef1234567890".to_string(),
        ];
        let redacted = redact_argv(&argv, &["ISOLATOR_TEST_SECRET_B".to_string()]);
        assert_eq!(redacted[0], "curl");
        assert!(redacted[2].contains("***REDACTED:ISOLATOR_TEST_SECRET_B***"));
        assert!(!redacted[2].contains("ghp_abcdef1234567890"));
        std::env::remove_var("ISOLATOR_TEST_SECRET_B");
    }
}

/// Log one `isolator run`/`shell`/`new`-driven command. `kind` lets
/// callers flag a command as something more specific than a routine
/// exec — e.g. `run_cmd` tags anything that looks like `git push` as
/// "git-push" instead of "exec", since that's the one channel through
/// which code actually leaves the sandbox (see docs/THREAT-MODEL.md,
/// "why not a pre-push hook").
pub fn log_exec(project: &str, m: &Manifest, kind: &str, argv: &[String], exit_code: Option<i32>) -> Result<()> {
    paths::ensure_project_dirs(project)?;
    let path = paths::chain_log_path(project)?;
    let data = json!({
        "project": project,
        "argv": redact_argv(argv, &m.secrets),
        "exit_code": exit_code,
    });
    append_chained(&path, kind, data)?;
    Ok(())
}

/// Fold any new lines from the egress gateway's own access log into the
/// project's audit chain. Tracks how many raw lines have already been
/// folded in `audit/.egress-offset` so re-running `isolator audit` is
/// idempotent. If the egress container was restarted (its log lives on
/// tmpfs and resets), the offset is detected as stale and reset rather
/// than silently under- or over-counting.
pub fn fold_egress_log(project: &str, m: &Manifest) -> Result<usize> {
    let (status, out) = crate::proc::run_capture(
        "docker",
        &["exec", &m.egress_container(), "cat", "/var/log/tinyproxy/access.log"],
    )?;
    if !status.success() {
        // Egress container not running / log not there yet — nothing to fold.
        return Ok(0);
    }

    let offset_path = paths::egress_offset_path(project)?;
    let prev_offset: usize = std::fs::read_to_string(&offset_path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    let lines: Vec<&str> = out.lines().collect();
    let start = if prev_offset > lines.len() { 0 } else { prev_offset };

    let chain_path = paths::chain_log_path(project)?;
    let new_text = lines[start..].join("\n");
    let mut folded = 0;
    for event in crate::egress_log::parse_log(&new_text) {
        let kind = match event.severity {
            crate::egress_log::Severity::Tripwire => "tripwire",
            crate::egress_log::Severity::Normal => "egress",
        };
        append_chained(&chain_path, kind, serde_json::to_value(&event)?)?;
        folded += 1;
    }

    if let Some(parent) = offset_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&offset_path, lines.len().to_string())?;
    Ok(folded)
}
