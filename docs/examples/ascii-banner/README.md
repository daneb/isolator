# Example: an ASCII-banner CLI, built end to end by a live agent

`banner.js` here is not a hand-written sample — it's the exact output of
a real Claude Code agent, authenticated via a claude.ai subscription
(`CLAUDE_CODE_OAUTH_TOKEN`, per
[ADR-0002](../decisions/0002-claude-code-authentication.md)), building a
small utility fully inside an isolator sandbox:

```bash
isolator new banner-demo --image isolator/node:latest
isolator secrets set banner-demo CLAUDE_CODE_OAUTH_TOKEN
isolator run banner-demo -- claude --print --dangerously-skip-permissions \
  "In /workspace, build a small Node.js CLI utility called banner.js ..."
```

This exercise served as a live validation of the sandbox itself — not
just "does the utility work," but "does the isolation, egress control,
and audit trail hold up against a real, autonomous coding agent."

## What it does

`banner.js` takes text (CLI args or stdin), uppercases it, strips ASCII
control characters (0x00–0x1F except newline, and 0x7F — this text gets
echoed to a terminal, so raw escape sequences from arbitrary input must
never reach it), and prints it inside a bordered box, wrapped at 100
columns. Zero dependencies. `npm test` runs its `node:test` suite.

## Two real bugs this run found (fixed, not hypothetical)

1. **Every Bash tool call inside the sandbox failed.** The sandbox's
   `read_only: true` rootfs only had named volumes at `/workspace` and
   `/home/agent/.cache` — Claude Code needs to write to
   `/home/agent/.claude` for its own session bookkeeping, so every Bash
   invocation died with `EROFS`. Fixed by adding a `claude-state` named
   volume (still no host bind mount) in
   [compose/project.compose.yml.tmpl](../../compose/project.compose.yml.tmpl),
   *and* pre-creating `/home/agent/.claude` with correct `agent`
   ownership in [images/base/Dockerfile](../../images/base/Dockerfile) —
   a fresh named volume's initial ownership comes from whatever already
   exists at that path in the image, and that directory didn't exist
   there at all before this fix, so Docker created the mount point
   root-owned instead.
2. **`isolator secrets set` had no feedback loop.** Terminal echo is
   disabled during entry (so a token isn't visible on-screen), but with
   no confirmation that a paste registered, a real paste got entered
   three times and silently concatenated into one corrupted 324-character
   value (the real token is 108) — `security add-generic-password`
   accepted it without complaint, and auth then failed opaquely, much
   later, with `401 OAuth access token is invalid`. Fixed in
   [cli/src/secrets.rs](../../cli/src/secrets.rs): `secrets set` now
   echoes back a character count (never the value) immediately after
   storing, so a corrupted paste is obvious on the spot.

## Security verification performed against the live container

- `isolator selftest banner-demo` — all 8 static hardening checks
  (read-only rootfs, `cap_drop: ALL`, `no-new-privileges`, non-root user,
  no bind mounts, non-default network, pids limit) and all 3 active
  breakout probes (canary-domain reachability, read-only-fs write,
  `docker.sock` presence) passed.
- Manual breakout attempts beyond the built-in battery — all blocked:
  reaching a non-allowlisted domain (`example.com`, DNS unresolvable —
  the sandbox network has no route out at all except through the egress
  proxy), writing outside `/workspace` (`/etc`, read-only filesystem),
  privilege escalation (no `sudo` binary present), and searching the
  entire filesystem for `docker.sock` (absent). An allowlisted domain
  (`github.com`) still worked, confirming this is genuine domain
  filtering, not a broken network.
- **An unprompted, real example of the egress control working**: the
  audit log shows Claude Code itself attempting to reach
  `http-intake.logs.us5.datadoghq.com` (its own telemetry) mid-run —
  correctly denied, because that domain was never added to the
  allow-list. Nobody engineered this; it's what happened.
- `isolator audit banner-demo --verify` — all 44 entries verified
  intact, hash chain unbroken. Every exec (including the two full
  `claude --print` invocations, verbatim), every git command, the
  selftest's tripwire-check, and every egress decision (including the
  Datadog denial and the canary-domain tripwire) is in the trail.

No isolator code change came out of the security-verification pass
itself — every control held. The two bug fixes above came from actually
running the intended workflow, not from the security checks.
