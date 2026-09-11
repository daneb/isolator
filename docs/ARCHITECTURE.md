# Architecture

```
Host (Mac Mini, OrbStack/Docker)
 └─ isolator (Rust CLI)                     ← the only thing you run on host
     ├─ ~/.isolator/projects/<name>/isolator.yaml   (manifest: allowlist, limits, secret refs)
     ├─ ~/.isolator/projects/<name>/audit/*.jsonl    (host-only-writable, append-only)
     └─ docker compose per project:
         ├─ <name>-sandbox   container   ← keel + agent CLIs + toolchain run here
         │     workspace = named volume <name>-workspace (no host mount)
         │     network   = <name>-net (custom bridge, egress via proxy only)
         │     non-root user, read-only rootfs, cap-drop ALL, no-new-privileges,
         │     seccomp default, pids/mem/cpu limits, no docker.sock
         └─ <name>-egress    container   ← forward proxy, domain allowlist,
               logs every request, holds the canary token watch
```

## Components

### `isolator` (host, trusted)

A Rust CLI (`cli/`) that is the only thing the operator runs directly on
the Mac Mini for project work. It shells out to `docker`, `docker compose`
and `gh` via `std::process::Command` — the same subprocess pattern keel
uses for its drivers (own process group, captured stdout/stderr, no shell
interpolation of untrusted strings). It owns:

- `~/.isolator/projects/<name>/isolator.yaml` — the per-project manifest:
  egress allow-list, resource limits, referenced (not valued) secret
  names, base image choice.
- `~/.isolator/projects/<name>/audit/*.jsonl` — append-only audit log,
  written only by the host process. No container ever has this path
  mounted.

### Sandbox container (untrusted workload)

Built from `images/base` (+ a language layer from `images/<lang>`).
Contains: `keel`, the configured agent CLI (`claude`, later others), git,
common dev tools. Runs as a fixed non-root UID. Its only writable
filesystem is a named Docker volume (`<name>-workspace`) plus `/tmp`
(tmpfs) and a small package-cache overlay — the container root filesystem
itself is read-only. It has **no bind mount to any host path** and never
sees `docker.sock`.

keel runs entirely inside this container and drives the agent CLI exactly
as it does on a bare host today (see keel's `SECURITY.md` — keel assumes
a trusted host and does no sandboxing itself; this container *is* that
trust boundary instead of the Mac Mini).

### Egress container (untrusted-facing, isolator-controlled)

A forward proxy that is the sandbox's only route to the internet
(`HTTP_PROXY`/`HTTPS_PROXY`, plus network-level enforcement so a process
can't just ignore the env vars). Allows CONNECT/requests only to hostnames
listed in the project's `isolator.yaml`. Does not terminate TLS — it
allow-lists by SNI/CONNECT target, so the agent's connection to
`api.anthropic.com` or `github.com` stays end-to-end encrypted. Logs every
request (domain, verdict, size, timestamp) and watches every request body
for the planted canary token.

### Audit pipeline

One hash-chained, append-only file per project:
`~/.isolator/projects/<name>/audit/chain.jsonl`. Every entry has a `kind`
(`exec`, `git-push`, `egress`, `tripwire`, `tripwire-check`), a
timestamp, and a `hash` that commits to the entry's own contents plus the
previous entry's hash — a Merkle-style chain, not just a log file. Three
things append to it:

1. **Exec entries** — every `isolator run`/`shell` invocation (redacted
   argv, exit code), written directly by the CLI as it happens. A command
   that looks like `git push` is tagged `kind: "git-push"` instead of the
   generic `exec`, since that's the one channel through which code
   actually leaves the sandbox for real.
2. **Egress entries** — folded in from the egress gateway's own access
   log by `isolator audit` / `isolator selftest` (idempotent — a
   `.egress-offset` checkpoint tracks how much has already been folded).
   A request to the reserved canary domain folds in as `kind: "tripwire"`
   instead of routine `egress`.
3. **Tripwire-check entries** — a marker written each time `isolator
   selftest`'s active breakout battery runs (canary-domain reachability,
   read-only-filesystem write attempt, `docker.sock` presence).

`isolator audit <name> --verify` recomputes the whole chain and reports
exactly which entry (if any) has been edited, deleted, reordered, or
forged — see docs/THREAT-MODEL.md's "audit tampering" section.
`isolator audit <name> --export <dir>` bundles `chain.jsonl` plus
keel's own evidence trail (`.keel/store/evidence/*/bundle.tar.gz`,
pulled out of the workspace volume via `docker cp` — that data is keel's
job, not isolator's, so isolator only ever reads it, never generates it)
into one `tar.gz` for review.

Secret values known to the `isolator` process's own environment (per the
manifest's `secrets:` list) are redacted from every chain entry before
it's written — see `audit::redact` and its stated limits in
docs/THREAT-MODEL.md.

## Why no bind mount

The container never has a host path mounted into it — not the project
folder, not `$HOME`, nothing. This is the one place strict isolation was
chosen over convenience: project source lives only in a Docker-managed
volume, and is reached from the host only via `isolator shell` (an
interactive `docker exec`) or by `git push` through the egress proxy. See
[THREAT-MODEL.md](THREAT-MODEL.md) for the reasoning.
