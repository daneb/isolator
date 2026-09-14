# ADR-0007: sync the sandbox's git identity from the host

**Status:** Accepted
**Date:** 2026-09-14

## Context

`keel approve` records who approved a stage:

```
moor run keel -- keel approve blast-radius --stage spec
  approved spec stage of `blast-radius` as unknown (8fd333590157)
```

**"unknown."** Traced into keel's own source
(`~/Repos/keel/src/approval.rs::current_user`): it runs `git config
user.name` in the repo, and if that's empty or the command itself fails,
falls back to `$USER`/`$USERNAME`, and only says "unknown" if both come
up empty. Checked directly against a live container:

```
$ moor run keel -- git config user.name    # empty, command exits 1
$ moor run keel -- printenv USER            # unset, exits 1
$ moor run keel -- whoami                   # "agent"
```

Neither is set. The base image (`images/base/Dockerfile`) never
configures git identity — there was no reason to, until keel's own
approval trail needed one. The host, meanwhile, already has one:
`git config --get user.name` → "Dane Balia".

## Decision

`commands::mod::sync_git_identity(m: &Manifest)` reads the *host's*
`git config --get user.name`/`user.email` and, if set, writes the same
values into the container via `docker exec ... git config user.name
<value>` — deliberately **not** `--global`: the container's rootfs is
read-only outside its mounted volumes (a checked hardening control, see
`moor selftest`), so `~/.gitconfig` can't be written at all; only the
repo-local `.git/config`, which lives on the writable `/workspace`
volume, actually succeeds. Silent and best-effort throughout — a
missing repo or an unset host git identity just means nothing happens,
never a failure that blocks `up`/`new`/`import`.

Called from three places, because "the repo exists yet" isn't true at
the same point in every flow:

- `compose_up` (shared by `new`/`up`/`import`) — covers `moor up` on an
  already-existing project, and keeps it in sync if the host's own git
  config ever changes.
- `new_cmd::run`, again, right after `keel init` (or the `--github`
  clone) — `compose_up`'s call earlier in the same flow is a guaranteed
  no-op, since `/workspace` isn't a git repo yet at that point.
- `import_cmd::run`, again, right after the bundle clone completes — a
  fresh `git clone` doesn't carry over the source repo's local config,
  so the imported repo starts with the same unset identity as a brand
  new one.

Verified live against the real `keel` project — before: `git config
user.name` in the sandbox was empty; after `moor up keel`: `Dane
Balia`/`happyfrog@tuta.io`, sourced from this Mac's own `git config`.
Approving a throwaway spec's stage then recorded `approved spec stage
of \`identity-check\` as Dane Balia`, not "unknown" — the throwaway spec
was removed afterward, this project's own audit trail is otherwise
untouched.

## What this deliberately does not do

- **No new config surface.** No `moor.yaml` field, no flag — the host's
  own `git config` is already the obvious source of truth, and an
  operator who wants something different can always run `moor run
  <name> -- git config user.name "..."` themselves afterward; this just
  removes the need to do that by hand every time.
- **Doesn't touch commit authorship as a design question on its own.**
  This fixes *who a human is recorded as* when they run `keel approve`
  — that's unambiguously the operator's own identity. Whether an
  autonomously-driven `keel run` build step should commit code as that
  same identity, a distinct "moor agent" identity, or with a
  co-authorship trailer (the way this project's own commits carry
  `Co-Authored-By: Claude Sonnet 5`) is a separate question this change
  doesn't answer — it only makes sure git identity is configured at
  all, which was previously missing outright.

## Consequences

- Git commits made inside the sandbox (by an operator via `moor
  shell`/`moor run`, or by `keel run`'s agent driver) now have a real
  author instead of failing outright or however `git commit` currently
  behaves with no identity configured at all — a side effect of fixing
  the audit trail, not the primary goal, but worth knowing about.
- If the host's own `git config user.name`/`user.email` is itself
  unset, the sandbox's stays unset too, and `keel approve` still says
  "unknown" — this syncs an identity, it doesn't invent one.
