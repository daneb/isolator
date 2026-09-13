# moor

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
`container` tool, and
[docs/decisions/0002-claude-code-authentication.md](docs/decisions/0002-claude-code-authentication.md)
for why Claude Code auth is an injected token rather than a mounted
`~/.claude`,
[docs/decisions/0003-claude-code-permissions.md](docs/decisions/0003-claude-code-permissions.md)
for why every project defaults to Claude Code's own `auto` permission
mode instead of `--dangerously-skip-permissions`,
[docs/decisions/0004-rename-to-moor.md](docs/decisions/0004-rename-to-moor.md)
for why this was `isolator` and is now `moor`, and
[docs/examples/ascii-banner](docs/examples/ascii-banner) for a small
utility built end to end by a real Claude Code agent running inside a
sandbox — including two real bugs that run found and fixed, and the
full security/audit verification against the live container.

[docs/CI.md](docs/CI.md) covers what runs in `.github/workflows/ci.yml`
on every push/PR — Rust lint/test/audit/deny, shellcheck, hadolint,
gitleaks, a Trivy CVE scan of every built image, and the real
`tests/e2e.sh` against live containers — and, importantly, what each of
those actually found and fixed versus what's deliberately (and
narrowly) suppressed, with the reasoning inline.

## Quick start

```bash
# one-time: build the sandbox + egress images
./images/build.sh

# one-time: build the moor CLI
cd cli && cargo build --release
# (or use ./target/debug/moor during development)

# create a project (add --github to also create a private GitHub repo)
moor new my-app --image moor/node:latest

# ...or bring an existing project in — history and all, no bind mount
moor import keel --from ~/Repos/keel

# work inside it
moor shell my-app
moor run my-app -- keel status

# check the sandbox is actually locked down the way it should be
moor selftest my-app

# see what's happened in this project so far
moor audit my-app

moor down my-app
```

Secrets (`ANTHROPIC_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`, `GITHUB_TOKEN`,
...) are read from your shell's environment if exported, otherwise from
the macOS Keychain — set one once and every future `moor up` just
picks it up, nothing to re-export each session:

```bash
moor secrets set my-app ANTHROPIC_API_KEY   # prompts, hides input where possible
moor secrets status my-app                  # where each declared secret resolves from
moor secrets unset my-app ANTHROPIC_API_KEY
```

If you pay for Claude via a claude.ai subscription rather than metered
API credits, use `CLAUDE_CODE_OAUTH_TOKEN` instead of `ANTHROPIC_API_KEY`
— the `claude` CLI inside the sandbox honors it the same way. Generate it
once on the host (this does the interactive login there, not in the
container) and store it the same way as any other secret:

```bash
claude setup-token                                    # one-time, on the host — opens a browser login
moor secrets set my-app CLAUDE_CODE_OAUTH_TOKEN   # paste the token it prints
```

Note: whether `keel` invokes the `claude` driver (vs. falling back to
`--no-driver` mode) is keel's own credential check, not moor's — see
[keel](https://github.com/daneb/keel)'s docs if it doesn't pick up
`CLAUDE_CODE_OAUTH_TOKEN` the same way it picks up `ANTHROPIC_API_KEY`.

## Layout

- `images/` — the sandbox base image + per-language layers (node, rust, python)
- `proxy/` — the egress gateway (default-deny forward proxy)
- `policies/` — the hardening baseline and the manifest schema, documented
- `cli/` — the `moor` Rust CLI, including `cli/templates/` (the
  per-project docker-compose template — lives inside the crate, not a
  top-level `compose/`, so a published crate can actually embed it)
- `docs/` — threat model and architecture
- `Makefile` — local dev tooling (`make help`); mirrors
  `.github/workflows/ci.yml` so `make ci` runs the same checks locally

## Security

### Why you can trust this

Nothing here asks you to take moor's word for it — every claim below
is either checked by code you can read, or was verified live against a
real container and written up with the evidence attached:

- **The core guarantee is structural, not configurable.** No host bind
  mount, no `docker.sock`, `cap_drop: ALL`, non-root, read-only rootfs,
  a genuinely internal Docker network with no route out except through
  the egress gateway — see [docs/THREAT-MODEL.md](docs/THREAT-MODEL.md)
  for why each one is there. `moor selftest` re-checks all of it
  against the *live* container (not just the manifest) every time you
  run it, and fails safe: a corrupted or missing `docker inspect` field
  reads as FAIL, never as "all clear" (`missing_fields_default_to_failing_safe`
  in `cli/src/commands/selftest.rs`).
- **The audit trail is tamper-evident, not just append-only.** Every
  entry hash-chains to the one before it; `moor audit --verify`
  recomputes the whole chain and names exactly which entry was edited,
  deleted, reordered, or forged if any was — proven with a live tamper
  test in `tests/e2e.sh`, not just asserted.
- **The agent's own permission checks are a second layer, not skipped.**
  Every project defaults to Claude Code's `auto` permission mode
  (`images/base/claude-settings.json`) instead of
  `--dangerously-skip-permissions` — verified directly against a live
  container: it creates files when asked and refuses a destructive
  `rm -rf` unprompted, with no human needed to answer a prompt. See
  [ADR-0003](docs/decisions/0003-claude-code-permissions.md). That
  means a compromised or manipulated agent run (the real risk category
  incidents like prompt-injection-driven tool abuse fall into) has to
  get past *two* independent things, not one: Claude's own per-call
  risk judgment, and the sandbox's structural containment below if that
  judgment is ever fooled.
- **Every design decision that matters is written down, including the
  ones that didn't go moor's way.** ADR-0001 explains why gVisor and
  Apple's `container` tool were both rejected (and exactly what would
  need to change for that to flip); ADR-0002 explains why Claude Code
  auth is an injected token rather than a mounted `~/.claude`, including
  a proposal that *was* made and rejected during review. Nothing about
  the threat model is asserted without the reasoning attached.
- **CI enforces this on every push, and was itself verified for real** —
  not just written and assumed to work. [docs/CI.md](docs/CI.md) covers
  Rust lint/audit/deny, shellcheck, hadolint, gitleaks, a Trivy CVE scan
  of every built image, and the full `tests/e2e.sh` battery; every job
  was run against real GitHub Actions before being called done, and the
  doc names the real bugs that surfaced doing it (a broken Bash tool
  under the read-only rootfs, a silently-corrupted secret from a
  triple-paste, an end-of-life Node runtime) rather than just the tools'
  names.
- **The repo is public.** The threat model, every hardening control, and
  every known gap below are readable by anyone, including someone
  deciding whether to attack it — the design has to hold up without
  relying on obscurity, and that's a deliberate choice, not an oversight.

### Open concerns — not yet closed out

Being direct about what isn't solved is part of the trust case above,
not separate from it:

- **No branch protection on `master`.** CI reports pass/fail; nothing
  currently *blocks* a push that fails it. Low-stakes for a single-
  operator repo today, but worth fixing before this has other
  committers.
- **No scheduled re-scan.** Trivy/`cargo audit`/`cargo deny` only run
  when code changes (`push`/`pull_request` triggers) — a CVE published
  against an already-built, unchanged image or dependency tree goes
  undetected until the next commit, which could be a long time.
- **No vulnerability disclosure process.** Public repo, no `SECURITY.md`
  — there's no documented way for someone who finds a real issue to
  report it privately instead of opening a public issue.
- **The shared-kernel risk is open, not mitigated.** Staying on `runc`
  (ADR-0001) means a kernel/runc syscall escape isn't defended in depth
  the way gVisor or per-container VMs would; the only current mitigation
  is keeping Docker/OrbStack current. gVisor was rejected because this
  platform can't run it today, not because the risk it addresses isn't
  real.
- **Debian CVEs with no fix yet stay in every image, indefinitely.**
  `trivy --ignore-unfixed` is the right CI gate (see docs/CI.md for why),
  but it deliberately doesn't fail on `affected`/`fix_deferred`/
  `will_not_fix` findings — real, currently-unpatchable exposure that
  the scan will never turn red for.
- **Secret values are briefly visible in one subprocess's `argv`.**
  `security add-generic-password -w <value>` (macOS Keychain writes) has
  no non-interactive API that avoids this — documented as an accepted
  tradeoff in `cli/src/secrets.rs`, not something moor's own code
  can route around.
- **No image signing, provenance, or SBOM.** Nothing today lets you
  cryptographically verify a built image matches this source, or hand
  someone a machine-readable bill of materials for one.
- **Auto mode's risk judgment is Anthropic's to maintain, not moor's
  to verify per-release.** [ADR-0003](docs/decisions/0003-claude-code-permissions.md)
  replaced the blanket `--dangerously-skip-permissions` default with
  Claude Code's own `auto` permission mode — a real improvement, checked
  directly against a live container — but that mode's actual judgment
  calls are a model behavior moor doesn't control and hasn't
  re-verified across every future Claude Code version. An operator can
  still pass `--dangerously-skip-permissions` explicitly for a given run
  if they want the old behavior back.
- **No independent review.** Everything above is self-assessed — this
  project's own docs, ADRs, CI, and one real walkthrough. No third-party
  penetration test or external security audit has been done.

### Where this goes next

- A scheduled (nightly/weekly) CI run of the Trivy/audit/deny jobs,
  independent of code changes, to catch newly disclosed CVEs against
  images that haven't otherwise changed.
- Branch protection requiring CI to pass before merge, once this has
  more than one committer.
- A `SECURITY.md` with an actual disclosure contact/process.
- Image signing (cosign/sigstore) and SBOM generation (syft), published
  alongside each build.
- Revisit gVisor if a self-managed Linux host is ever justified; revisit
  Apple's `container` tool if
  [apple/container#719](https://github.com/apple/container/discussions/719)
  (the host-gateway egress leak) closes with a real fix.
- A custom seccomp profile derived from real `strace` output of an
  actual toolchain run, replacing the current default profile (already
  flagged as deferred in `policies/README.md`).
- A larger active breakout battery in `moor selftest` — today's
  three probes (canary domain, read-only fs, `docker.sock`) are a
  starting point, not a ceiling.
- Automated dependency updates (Dependabot/Renovate) for `Cargo.lock`
  and the toolchain versions baked into each image, instead of relying
  on someone remembering to bump them.
- Re-verify `auto` permission mode's actual behavior (does it still
  create files when asked, still refuse a destructive command
  unprompted) whenever the base image bumps its Claude Code version —
  it's a model behavior, not a pinned rule moor controls.

## Testing

- `cd cli && cargo test` — 59 unit tests: selftest's hardening evaluator
  (fed synthetic `docker inspect` JSON, including fail-safe-on-missing-data
  cases), manifest validation and round-tripping, compose template
  rendering, the tinyproxy access-log parser, the audit hash chain
  (append/verify, plus deliberately editing, deleting, reordering, and
  forging entries to confirm `--verify` catches each one), Keychain
  service-name scoping, and `moor import`'s image auto-detection and
  GitHub-URL parsing.
- `./tests/e2e.sh` — end-to-end against real Docker containers: creates a
  throwaway project, runs `moor selftest`'s static checks and active
  breakout battery, confirms egress allow/deny against github.com and
  example.com, confirms a `git push` attempt is tagged distinctly in the
  audit chain, folds the egress log in via `moor audit`, verifies the
  chain, **live-tampers with the real chain.jsonl file and confirms
  `--verify` detects it**, exports an audit bundle, and imports a
  throwaway local multi-branch repo end-to-end (image auto-detect, `keel
  init`, both branches present, selftest still passes). Requires the
  images to already be built (`./images/build.sh`).
- Both of the above, plus security/quality scanning (Rust lint/audit/deny,
  shellcheck, hadolint, gitleaks, Trivy image CVE scans), run in CI on
  every push/PR — see [docs/CI.md](docs/CI.md).
- `make help` lists every local dev target — `make check` for a fast
  fmt/clippy/test pass, `make security` for the full scan suite,
  `make ci` to run everything CI runs (including the real `tests/e2e.sh`)
  in one shot. Each security target checks its tool is installed and
  says how to get it (`brew install ...`) rather than failing on a bare
  "command not found." Every target here was run for real before being
  documented, not just written.

## Development

```bash
make help          # list every target
make check          # fast: fmt-check, clippy, cargo test — no Docker needed
make security        # cargo audit/deny, shellcheck, hadolint, gitleaks, trivy
make ci               # everything above plus the real tests/e2e.sh — the full CI replica
make build            # cargo build (debug)
make release          # cargo build --release
make install          # cargo install the CLI to ~/.cargo/bin
make clean            # cargo clean
```

`make publish-dry-run` runs `cargo publish --dry-run`. This crate was
originally named `isolator` and marked `publish = false` on purpose —
`isolator` is already taken on crates.io by an unrelated package, and
the crate wasn't meant to be published at all at the time. Three things
were needed to actually fix that, not just the first one that looked
like the whole story: renamed to `moor` (available, checked directly
against the crates.io API — see
[ADR-0004](docs/decisions/0004-rename-to-moor.md) for the naming
process and every alternative tried), MIT-licensed
(`LICENSE` + `license = "MIT"` in `cli/Cargo.toml`), and — found only by
actually running `cargo publish --dry-run --allow-dirty` all the way
through, not stopping at the first passing check —
`cli/src/compose.rs`'s `include_str!` moved from a top-level `compose/`
into `cli/templates/`, since a published crate can't embed a file that
lives outside its own package directory. `cargo publish --dry-run` now
packages, compiles from the isolated package tarball, and gets to the
upload step (aborted only because it's a dry run) — verified, not
assumed.

`make publish` runs the real thing. It's still a deliberate manual step
for whoever owns the crates.io account — nothing here runs it for
you — but it exists now: it re-runs `publish-dry-run` first, then
requires typing the literal word `publish` at a prompt (not just
pressing enter) before calling `cargo publish` itself, since a crates.io
version can be yanked afterward but never deleted or reused. Needs
`cargo login` done once beforehand, with a token from
https://crates.io/settings/tokens. One caveat ADR-0004 is explicit about:
`cargo install moor` only brings the compiled binary, not `images/`,
`proxy/`, or `policies/` — without those (from a full
clone, with `./images/build.sh` run once) the installed binary has no
sandbox images to build from.

## Status

Phases 0–9 of the plan are done, with Phase 9 partial by design (see
below):

- **0–3**: threat model/architecture docs, base + language images, the
  egress gateway with a default-deny allow-list, the container hardening
  baseline (read-only rootfs, dropped capabilities, no bind mounts,
  non-root user, no default-bridge network).
- **4**: the `moor` CLI (`new`, `up`, `down`, `shell`, `run`, `status`,
  `audit`, `selftest`).
- **5**: keel + the Claude Code CLI verified running inside the sandbox
  (`keel init`, `keel run` all execute there, not on the host).
- **6**: a hash-chained, tamper-evident audit trail per project
  (`audit/chain.jsonl`) folding in exec commands, egress verdicts, and a
  distinct `git-push` tag; `moor audit --verify` and `--export`.
- **7**: `moor selftest` now also runs an active breakout battery
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

- **9**: Keychain-sourced secrets are built (`moor secrets
  set`/`unset`/`status`; resolved automatically by `up`/`run`/`shell`,
  ahead of the operator's own shell env, with redaction wired through so
  a Keychain-only secret still gets scrubbed from the audit chain — this
  closed a real gap: it originally worked only within the single process
  that first resolved the secret, not later `moor run` invocations).
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

- **Migration**: [`moor import`](docs/MIGRATING.md) brings an
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
