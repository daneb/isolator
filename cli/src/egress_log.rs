use crate::canary::CANARY_DOMAIN;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Verdict {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Severity {
    /// A routine allow, or a deny of a domain nobody should expect to
    /// reach — normal default-deny noise.
    Normal,
    /// A deny (or, if it ever happened, an allow) of the reserved canary
    /// domain: there is no legitimate reason for the sandbox to have
    /// tried this at all.
    Tripwire,
}

#[derive(Debug, Clone, Serialize)]
pub struct EgressEvent {
    pub verdict: Verdict,
    pub domain: String,
    pub severity: Severity,
    pub raw: String,
}

/// Parse one tinyproxy access-log line into a structured event, if it's
/// one of the two lines that carry a verdict. Every other line (startup
/// banter, connection-open/close bookkeeping) is ignored — pure function,
/// no I/O, so it's unit-testable against real captured log lines without
/// Docker.
pub fn parse_line(line: &str) -> Option<EgressEvent> {
    if let Some(domain) = extract_quoted_after(line, "Proxying refused on filtered domain \"") {
        return Some(classify(Verdict::Deny, domain, line));
    }
    if let Some(domain) = extract_quoted_after(line, "Established connection to host \"") {
        return Some(classify(Verdict::Allow, domain, line));
    }
    None
}

fn classify(verdict: Verdict, domain: String, raw: &str) -> EgressEvent {
    let severity = if domain.eq_ignore_ascii_case(CANARY_DOMAIN) {
        Severity::Tripwire
    } else {
        Severity::Normal
    };
    EgressEvent {
        verdict,
        domain,
        severity,
        raw: raw.to_string(),
    }
}

fn extract_quoted_after(line: &str, marker: &str) -> Option<String> {
    let start = line.find(marker)? + marker.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Parse every line of a tinyproxy access log, keeping only lines with a
/// verdict.
pub fn parse_log(text: &str) -> Vec<EgressEvent> {
    text.lines().filter_map(parse_line).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_allowed_connection() {
        let line = r#"CONNECT   Sep 11 19:44:10.896 [1]: Established connection to host "github.com" using file descriptor 6."#;
        let ev = parse_line(line).expect("should parse");
        assert_eq!(ev.verdict, Verdict::Allow);
        assert_eq!(ev.domain, "github.com");
        assert_eq!(ev.severity, Severity::Normal);
    }

    #[test]
    fn parses_denied_connection() {
        let line = r#"NOTICE    Sep 11 19:44:11.177 [1]: Proxying refused on filtered domain "example.com""#;
        let ev = parse_line(line).expect("should parse");
        assert_eq!(ev.verdict, Verdict::Deny);
        assert_eq!(ev.domain, "example.com");
        assert_eq!(ev.severity, Severity::Normal);
    }

    #[test]
    fn canary_domain_denial_is_flagged_as_tripwire() {
        let line = format!(
            r#"NOTICE    Sep 11 19:44:11.177 [1]: Proxying refused on filtered domain "{CANARY_DOMAIN}""#
        );
        let ev = parse_line(&line).expect("should parse");
        assert_eq!(ev.severity, Severity::Tripwire);
    }

    #[test]
    fn canary_domain_match_is_case_insensitive() {
        let line = format!(
            r#"NOTICE    Sep 11 19:44:11.177 [1]: Proxying refused on filtered domain "{}""#,
            CANARY_DOMAIN.to_uppercase()
        );
        let ev = parse_line(&line).expect("should parse");
        assert_eq!(ev.severity, Severity::Tripwire);
    }

    #[test]
    fn ignores_lines_without_a_verdict() {
        let lines = [
            "INFO      Sep 11 19:44:10.873 [1]: Starting main loop. Accepting connections.",
            r#"CONNECT   Sep 11 19:44:10.873 [1]: Connect (file descriptor 5): 192.168.107.3"#,
            r#"CONNECT   Sep 11 19:44:10.873 [1]: Request (file descriptor 5): CONNECT github.com:443 HTTP/1.1"#,
        ];
        for line in lines {
            assert!(parse_line(line).is_none(), "expected no event for: {line}");
        }
    }

    #[test]
    fn parse_log_extracts_only_verdict_lines_in_order() {
        let text = [
            "INFO      startup",
            r#"CONNECT   [1]: Established connection to host "github.com" using file descriptor 6."#,
            r#"NOTICE    [1]: Proxying refused on filtered domain "example.com""#,
            "INFO      shutdown",
        ]
        .join("\n");
        let events = parse_log(&text);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].domain, "github.com");
        assert_eq!(events[1].domain, "example.com");
    }
}
