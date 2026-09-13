# ADR-0004: Rename the crate/CLI from `isolator` to `moor`; MIT license

**Status:** Accepted
**Date:** 2026-09-13

## Context

This project wanted to actually `cargo publish`. Three things blocked
it — checked directly, one at a time, rather than assumed fixed after
the first one that looked like the whole story:

1. `cargo publish --dry-run` refused outright: `cli/Cargo.toml` had
   `package.publish = false`, set deliberately (ADR-0003's neighbor
   decision) so `cargo deny`'s license check wouldn't need an invented
   license for a crate that was never meant to be published.
2. The name `isolator` is already taken on crates.io — an unrelated
   crate ("A lightweight library for isolating Rust functions", v0.1.2)
   has owned it since January 2025.
3. Only found by getting past both of the above and actually running
   `cargo publish --dry-run --allow-dirty` all the way through (the
   plain `--dry-run`, without `--allow-dirty`, stops at the git-clean
   check before ever reaching this): `cli/src/compose.rs` had
   `include_str!("../../compose/project.compose.yml.tmpl")`, reaching
   *outside* the crate directory into a top-level `compose/`. A
   published crate can only contain files inside its own package
   directory — `include_str!` on a path that escapes it can never
   compile from the tarball `cargo publish` actually uploads, no matter
   how correct the name and license are.

Separately, `isolator` (8 characters) is long for a command a person
types constantly (`isolator new`, `isolator run`, `isolator shell`,
...). A short name matters more here than for most CLIs, since every
subcommand pays that cost on every invocation.

## Naming

Checked against the live crates.io API (`GET
/api/v1/crates/<name>`), not guessed. Nautical terms fitting the
existing `keel` ecosystem were tried in order of how well they matched
what this tool actually does (secure/contain, not just "ship-related"):
`brig`, `hull`, `hold`, `berth`, `moor`, `citadel`, `keep`, `cask`,
`bilge`, `caisson`, `hulk`, `moat`, `cage`, `vault`, `coffer`, `galley`,
`rigging`, `ballast`, `trim`, `heave`, `stow`, `quay`, `wharf`,
`anchor`, `lash`, `batten`, `cleat`, `tether`, `brace`, `stern`,
`hawser`, `capstan` — all taken except `moor`, `heave`, `batten`, and
`stern`. `moor` was chosen: securing a vessel in a fixed, contained
place is close to what this tool does to an agent's workspace, and it's
four letters.

## Decision

- Crate and binary: `isolator` → `moor` (`cli/Cargo.toml`,
  `cli/src/main.rs`'s clap command name).
- License: MIT, `LICENSE` at the repo root, `license = "MIT"` in
  `cli/Cargo.toml`. `cli/deny.toml`'s `licenses.private.ignore` flipped
  back to `false` — the crate has a real license now, so the check runs
  for real instead of being skipped.
- Every host-visible identifier renamed to match, not just the crate:
  `~/.isolator/` → `~/.moor/`, `isolator.yaml` → `moor.yaml`, the
  Keychain account/service-name prefix (`isolator` → `moor`), the
  canary domain (`canary.isolator.invalid` → `canary.moor.invalid`) and
  env var (`ISOLATOR_CANARY_TOKEN` → `MOOR_CANARY_TOKEN`), and every
  Docker image tag (`isolator/base:latest` → `moor/base:latest`, etc.).
  A crate name and its installed binary name don't have to match, but
  leaving the actual command as `isolator` would have missed the actual
  point (a short, frequently-typed command) for the sake of a
  registry-only rename.
- `cli/templates/project.compose.yml.tmpl` (moved from a top-level
  `compose/`) is now the crate's own copy — `include_str!` reads it as
  `"../templates/project.compose.yml.tmpl"`, relative to
  `cli/src/compose.rs`. One canonical copy, inside the package, not a
  duplicate kept in sync by hand.
- The GitHub repository itself (`daneb/isolator`) and the real, separate
  `daneb/isolator-sample-app` repository from
  [docs/WALKTHROUGH.md](../WALKTHROUGH.md) were, at the time of this
  decision, deliberately left un-renamed — both are real external
  artifacts with their own URLs, unrelated to what the CLI binary is
  called. **Superseded the same day — see Update below:** the operator
  renamed both on GitHub anyway, independently of this ADR.

## Consequences

- Anyone with an existing `~/.isolator/` (this machine had two real
  projects: `banner-demo`, `isolator-sample-app`) needs a migration
  step, not a silent break — see the commit that lands alongside this
  ADR for exactly what that did: moved `~/.isolator/` → `~/.moor/`
  (a plain directory move — audit chain files are hash-chained by
  content, so moving them doesn't touch the chain), copied each
  project's Keychain secrets from the `isolator-*` service-name prefix
  to `moor-*` (by value, without ever displaying the value), and
  rebuilt every Docker image under the `moor/*` tag before re-upping
  each project.
- `cargo publish --dry-run --allow-dirty` now packages the crate,
  compiles it from the isolated package tarball (not the working
  directory — this is what actually caught the `include_str!` path
  issue), and reaches the upload step, aborted only because it's a dry
  run. The actual `cargo publish` (crates.io login, the irreversible
  part) is a deliberate manual step for the operator, not run here.
- `cargo install moor` only installs the compiled binary — not
  `images/`, `proxy/`, or `policies/`. Without those (and without
  `./images/build.sh` having been run from a full clone), the installed
  binary has no images to build sandboxes from. Publishing the crate
  doesn't change this; it was true before the rename too.

## Update (same day): the GitHub repositories were renamed too

The operator renamed both `daneb/isolator` → `daneb/moor` and
`daneb/isolator-sample-app` → `daneb/moor-sample-app` on GitHub,
independently of the "not renamed" call above — confirmed by querying
each old name against the GitHub API and reading back the redirected
`full_name`, not assumed. Updated to match:

- This repo's own `origin` remote (`git remote set-url`).
- `cli/Cargo.toml`'s `repository` field.
- Every doc link pointing at the old URLs (README, `docs/index.html`,
  this ADR).
- The `isolator-sample-app` *local* project's manifest `github_repo`
  field and its sandbox's own internal git `origin` remote (verified
  with a real `git fetch` against the new URL afterward) — chosen over
  also renaming the local project itself, since GitHub's redirect made
  that optional and the operator preferred to leave the local project
  name as-is.

`docs/WALKTHROUGH.md`'s command examples still show `isolator-sample-app`
as the local project name, since that's what was actually typed at the
time and remains the real local name today — only the GitHub repository
it points at changed.

