# isolator

Containerized, [keel](https://github.com/daneb/keel)-driven sandboxes for
AI coding agents. Every project gets its own Docker sandbox with no host
bind mount, no `docker.sock`, and a default-deny egress proxy — the agent
(Claude Code today) and keel both run entirely inside the container, so a
misbehaving agent or a malicious cloned repo can compromise at worst one
throwaway container, never the Mac Mini it runs on.

See [docs/THREAT-MODEL.md](docs/THREAT-MODEL.md) and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full design, and the
project plan for the phase breakdown this was built against.

## Quick start

```bash
# one-time: build the sandbox + egress images
./images/build.sh

# one-time: build the isolator CLI
cd cli && cargo build --release
# (or use ./target/debug/isolator during development)

# create a project (add --github to also create a private GitHub repo)
isolator new my-app --image isolator/node:latest

# work inside it
isolator shell my-app
isolator run my-app -- keel status

# check the sandbox is actually locked down the way it should be
isolator selftest my-app

# see what's happened in this project so far
isolator audit my-app

isolator down my-app
```

## Layout

- `images/` — the sandbox base image + per-language layers (node, rust, python)
- `proxy/` — the egress gateway (default-deny forward proxy)
- `compose/` — the per-project docker-compose template
- `policies/` — the hardening baseline and the manifest schema, documented
- `cli/` — the `isolator` Rust CLI
- `docs/` — threat model and architecture

## Status

Phases 0–4 of the plan are done: base + language images, the egress
gateway with a default-deny allow-list, the container hardening baseline
(read-only rootfs, dropped capabilities, no bind mounts, non-root user, no
default-bridge network — all asserted by `isolator selftest`), and the
`isolator` CLI (`new`, `up`, `down`, `shell`, `run`, `status`, `audit`,
`selftest`). Verified end-to-end: `keel` and the Claude Code CLI both run
inside the sandbox, `keel init` succeeds, and the egress proxy correctly
allows github.com/api.anthropic.com while blocking everything else by
default.

Not yet built: folding the egress proxy's log and keel's own evidence
bundles into one audit trail (Phase 6), the tripwire/breakout-detection
battery beyond the current hardening checks (Phase 7), and the full
ideation-to-shipped walkthrough doc (Phase 8).
