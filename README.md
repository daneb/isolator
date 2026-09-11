# isolator

Containerized, [keel](https://github.com/daneb/keel)-driven sandboxes for
AI coding agents. Every project gets its own Docker sandbox with no host
bind mount, no `docker.sock`, and a default-deny egress proxy — the agent
(Claude Code today) and keel both run entirely inside the container, so a
misbehaving agent or a malicious cloned repo can compromise at worst one
throwaway container, never the Mac Mini it runs on.

See [docs/THREAT-MODEL.md](docs/THREAT-MODEL.md) and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full design, and
[docs/WALKTHROUGH.md](docs/WALKTHROUGH.md) for a real ideation-to-shipped
run against an actual GitHub repo — including the three bugs it found.

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

Secrets (`ANTHROPIC_API_KEY`, `GITHUB_TOKEN`, ...) are read from your
shell's environment if exported, otherwise from the macOS Keychain — set
one once and every future `isolator up` just picks it up, nothing to
re-export each session:

```bash
isolator secrets set my-app ANTHROPIC_API_KEY   # prompts, hides input where possible
isolator secrets status my-app                  # where each declared secret resolves from
isolator secrets unset my-app ANTHROPIC_API_KEY
```

## Layout

- `images/` — the sandbox base image + per-language layers (node, rust, python)
- `proxy/` — the egress gateway (default-deny forward proxy)
- `compose/` — the per-project docker-compose template
- `policies/` — the hardening baseline and the manifest schema, documented
- `cli/` — the `isolator` Rust CLI
- `docs/` — threat model and architecture

## Testing

- `cd cli && cargo test` — 50 unit tests: selftest's hardening evaluator
  (fed synthetic `docker inspect` JSON, including fail-safe-on-missing-data
  cases), manifest validation and round-tripping, compose template
  rendering, the tinyproxy access-log parser, the audit hash chain
  (append/verify, plus deliberately editing, deleting, reordering, and
  forging entries to confirm `--verify` catches each one), and Keychain
  service-name scoping.
- `./tests/e2e.sh` — end-to-end against real Docker containers: creates a
  throwaway project, runs `isolator selftest`'s static checks and active
  breakout battery, confirms egress allow/deny against github.com and
  example.com, confirms a `git push` attempt is tagged distinctly in the
  audit chain, folds the egress log in via `isolator audit`, verifies the
  chain, **live-tampers with the real chain.jsonl file and confirms
  `--verify` detects it**, and exports an audit bundle. Requires the
  images to already be built (`./images/build.sh`).

## Status

Phases 0–9 of the plan are done, with Phase 9 partial by design (see
below):

- **0–3**: threat model/architecture docs, base + language images, the
  egress gateway with a default-deny allow-list, the container hardening
  baseline (read-only rootfs, dropped capabilities, no bind mounts,
  non-root user, no default-bridge network).
- **4**: the `isolator` CLI (`new`, `up`, `down`, `shell`, `run`, `status`,
  `audit`, `selftest`).
- **5**: keel + the Claude Code CLI verified running inside the sandbox
  (`keel init`, `keel run` all execute there, not on the host).
- **6**: a hash-chained, tamper-evident audit trail per project
  (`audit/chain.jsonl`) folding in exec commands, egress verdicts, and a
  distinct `git-push` tag; `isolator audit --verify` and `--export`.
- **7**: `isolator selftest` now also runs an active breakout battery
  (canary-domain reachability, read-only-fs write attempt, `docker.sock`
  presence) against a live container, not just static `docker inspect`
  checks.
- **8**: [a real project run end-to-end](docs/WALKTHROUGH.md) — a private
  GitHub repo, a spec, two real gate-caught scope violations, a real
  `npm install`/`node --test`/`node --check` build inside the sandbox, a
  real `git push`, and a verified 51-entry audit trail. Found and fixed
  three real bugs: the private-repo clone had no credentials, the
  `--export` bundle pointed at a keel evidence path that doesn't exist,
  and (same root cause) the audit command's own help text repeated it.

- **9**: Keychain-sourced secrets are built (`isolator secrets
  set`/`unset`/`status`; resolved automatically by `up`/`run`/`shell`,
  ahead of the operator's own shell env, with redaction wired through so
  a Keychain-only secret still gets scrubbed from the audit chain — this
  closed a real gap: it originally worked only within the single process
  that first resolved the secret, not later `isolator run` invocations).
  Checked gVisor/runsc: not available under this OrbStack install (only
  `runc`), so still deferred — see `policies/README.md`'s Runtime row.
  Still deferred, genuinely not attempted: per-task ephemeral containers
  for keel `--waves`, and remote/centralized audit log shipping (no
  remote destination has been specified to ship to).
