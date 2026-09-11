use std::time::{SystemTime, UNIX_EPOCH};

/// A hostname reserved as a tripwire: it is never added to any project's
/// egress allow-list, so any request the sandbox makes to it is always a
/// deny — and folded into the audit trail as a TRIPWIRE, not a routine
/// deny, because there is no legitimate reason for it to ever be
/// attempted. See docs/THREAT-MODEL.md.
pub const CANARY_DOMAIN: &str = "canary.isolator.invalid";

/// A decoy value planted in the sandbox's environment as
/// `ISOLATOR_CANARY_TOKEN`. It looks like a credential but authenticates
/// nothing — its only purpose is to exist somewhere a real secret would,
/// so a probe (in `isolator selftest`'s breakout battery) can confirm it
/// never reaches an unapproved destination. See proxy/README.md for why
/// this is a secondary control, not the primary one (TLS isn't
/// intercepted, so payload-level scanning only really covers plain HTTP).
pub fn generate_token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("isolator-canary-{:x}-{:x}", nanos, std::process::id())
}
