# ADR-0002: Claude Code authenticates via an injected token, never a mounted `~/.claude`

**Status:** Accepted
**Date:** 2026-09-12

## Context

Claude Code runs inside the sandbox container (installed in
`images/base/Dockerfile`) and needs credentials. Two ways to get a
claude.ai *subscription* (rather than metered API-key billing) working
were considered.

## Options considered

### 1. Bind-mount the host's `~/.claude` directory read-only into the container

Rejected. isolator's threat model rests on one non-negotiable property:
**no host bind mount, ever** — see
[docs/THREAT-MODEL.md](../THREAT-MODEL.md) and the "Why no bind mount"
section of [docs/ARCHITECTURE.md](../ARCHITECTURE.md), enforced by the
`sandbox_has_no_bind_mount_volume_entries` test in `cli/src/compose.rs`
and checked live by `isolator selftest`. `~/.claude` also holds more
than one credential — other projects' session state, settings, and
potentially other stored auth — so mounting the whole directory into an
untrusted sandbox would expose strictly more than the sandbox needs,
for every project, permanently (a mount, not a one-time value). This
was proposed and explicitly rejected in favor of option 2 during
implementation review.

### 2. Inject a subscription-derived token through the existing secrets pipeline (chosen)

The `claude` CLI accepts a long-lived, non-interactive credential —
`CLAUDE_CODE_OAUTH_TOKEN`, generated once via `claude setup-token` — as
an alternative to `ANTHROPIC_API_KEY`. `claude setup-token` does its
interactive browser login **on the host**, never inside the container;
the container only ever sees the resulting token, exactly as narrow in
scope as an API key.

isolator's secret handling (`cli/src/manifest.rs`, `cli/src/secrets.rs`,
`cli/src/compose.rs`) is already generic over secret *names* — resolve
from shell env, else macOS Keychain, else leave unset; render into
`docker compose`'s `${VAR:-}` substitution; redact from the audit chain
by value. Adding `CLAUDE_CODE_OAUTH_TOKEN` needed no new mechanism, only
a new entry in the default secrets list (`Manifest::new`) — the same
pipeline `ANTHROPIC_API_KEY` and `GITHUB_TOKEN` already use.

## Decision

`CLAUDE_CODE_OAUTH_TOKEN` is a declared secret alongside
`ANTHROPIC_API_KEY`; either authenticates Claude Code inside the
sandbox. isolator never mounts a host credentials directory for this or
any other purpose.

```bash
claude setup-token                                  # one-time, on the host
isolator secrets set <project> CLAUDE_CODE_OAUTH_TOKEN
```

## Consequences

- No isolator code path ever bind-mounts a host directory for
  authentication — the no-bind-mount invariant holds without
  exception, including for this feature.
- Whether `keel`'s own driver-detection logic (which decides whether to
  invoke the `claude` driver vs. fall back to `--no-driver` mode)
  recognizes `CLAUDE_CODE_OAUTH_TOKEN` the same way it recognizes
  `ANTHROPIC_API_KEY` is `keel`'s own concern — a separate repository
  ([daneb/keel](https://github.com/daneb/keel)) — and unverified here.
- If a future credential type genuinely cannot be expressed as an env
  var, that would need its own ADR; it does not justify revisiting this
  one by default.
