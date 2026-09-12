# isolator

Containerized, [keel](https://github.com/daneb/keel)-driven sandboxes for
AI coding agents. Every project gets its own Docker sandbox with no host
bind mount, no `docker.sock`, and a default-deny egress proxy — the agent
(Claude Code today) and keel both run entirely inside the container, so a
misbehaving agent or a malicious cloned repo can compromise at worst one
throwaway container, never the Mac Mini it runs on.

See [docs/THREAT-MODEL.md](docs/THREAT-MODEL.md) and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full design,
[docs/WALKTHROUGH.md](docs/WALKTHROUGH.md) for a real ideation-to-shipped
run against an actual GitHub repo — including the three bugs it found —
[docs/MIGRATING.md](docs/MIGRATING.md) for bringing an existing project
(tested against keel's own repo) into a sandbox, and
[docs/decisions/0001-container-runtime-choice.md](docs/decisions/0001-container-runtime-choice.md)
for why this stays on `runc` rather than gVisor or Apple's native
`container` tool.

## Quick start

```bash
# one-time: build the sandbox + egress images
./images/build.sh

# one-time: build the isolator CLI
cd cli && cargo build --release
# (or use ./target/debug/isolator during development)

# create a project (add --github to also create a private GitHub repo)
isolator new my-app --image isolator/node:latest

# ...or bring an existing project in — history and all, no bind mount
isolator import keel --from ~/Repos/keel

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

- `cd cli && cargo test` — 59 unit tests: selftest's hardening evaluator
  (fed synthetic `docker inspect` JSON, including fail-safe-on-missing-data
  cases), manifest validation and round-tripping, compose template
  rendering, the tinyproxy access-log parser, the audit hash chain
  (append/verify, plus deliberately editing, deleting, reordering, and
  forging entries to confirm `--verify` catches each one), Keychain
  service-name scoping, and `isolator import`'s image auto-detection and
  GitHub-URL parsing.
- `./tests/e2e.sh` — end-to-end against real Docker containers: creates a
  throwaway project, runs `isolator selftest`'s static checks and active
  breakout battery, confirms egress allow/deny against github.com and
  example.com, confirms a `git push` attempt is tagged distinctly in the
  audit chain, folds the egress log in via `isolator audit`, verifies the
  chain, **live-tampers with the real chain.jsonl file and confirms
  `--verify` detects it**, exports an audit bundle, and imports a
  throwaway local multi-branch repo end-to-end (image auto-detect, `keel
  init`, both branches present, selftest still passes). Requires the
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
  `runc`), and switching to Docker Desktop wouldn't fix that either — its
  engine runs in the same kind of managed, non-administrable VM. Also
  looked at Apple's native `container` tool (its own-VM-per-container
  model would sidestep the "shared kernel" concern more fundamentally
  than gVisor does) — not usable on this Mac yet (needs a newer macOS
  than 15.3.1) and not a drop-in replacement for the `docker compose`
  layer this project is built on regardless. Both fully written up in
  `policies/README.md`'s Runtime rows.

  **Per-task ephemeral containers for `--waves` — deprioritized, not just
  deferred.** That item only matters if multiple keel tasks run
  concurrently in the same container, which only happens with `keel run
  --waves`. Daily usage here is serial (`gate g0` → `approve` → `gate g1`
  → `approve` → `run`, one stage at a time), where only one task is ever
  active in the sandbox at once — so there's no concurrent-task exposure
  to close today, and the canary token being per-project rather than
  per-task is a non-issue when there's only ever one task running. Revisit
  if `--waves` actually gets used.

  Still genuinely deferred: remote/centralized audit log shipping (no
  remote destination has been specified to ship to).

- **Migration**: [`isolator import`](docs/MIGRATING.md) brings an
  existing local project into a sandbox — history transferred via a
  one-shot `git bundle` (never a bind mount), image auto-detected,
  existing GitHub remote preserved. Verified against keel's own repo:
  exact commit history, both branches, and its already-existing
  `.keel/keel.toml` correctly left alone instead of overwritten. Found
  and fixed two real bugs building it: `docker cp` silently fails against
  a read-only rootfs even onto a writable tmpfs mount (fixed by streaming
  through `docker exec`'s stdin instead), and cleaning up a stale
  bundle-path `origin` remote was deleting every non-default branch along
  with it (fixed by materializing branches locally first).
