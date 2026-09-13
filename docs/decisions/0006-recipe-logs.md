# ADR-0006: `moor logs` — a live status view of a running recipe

**Status:** Accepted
**Date:** 2026-09-13

## Context

`moor recipe` (ADR-0005) drives keel's spec → gate → plan → gate → run
pipeline unattended between human checkpoints, but a step like `keel
run`'s real build can take a while, and `moor recipe`'s own progress
narration only ever went to the terminal that launched it — nothing
told you where it was at from a second terminal, over SSH, or after
the fact. The operator asked for exactly that: a trace or a view,
"doesn't have to be deep detail... events or critical events," to see
status and progress while keel is running.

## Decision

Reuse the existing hash-chained audit chain (`~/.moor/projects/<name>/audit/chain.jsonl`)
rather than inventing a second logging mechanism — the same principle
ADR-0005 already applied to approvals ("no new approval mechanism").
`moor recipe` now appends a short, structured entry (`kind:
"recipe-event"`) at each meaningful transition: recipe start, spec
scaffolded/resumed/authored, each stage `keel next` reports, each gate
attempt's pass/fail, a pause for approval, each `keel run` attempt, and
completion or failure. Fields are deliberately small (slug, stage,
attempt counters, pass/fail) — never captured command output, which is
already what the chain's ordinary `"recipe"`-kind exec entries hold.

`moor logs <name> [--follow] [-n <count>]` reads only the
`recipe-event` entries and renders them as short, timestamped one-line
status messages — e.g. `[17:31:07] [logs-verify] gate attempt 2/4:
fail`. `--follow` polls the file every 500ms and prints new lines as
they're appended, so a second terminal watching `moor logs --follow`
picks up a stage transition within half a second of it happening in
the terminal actually driving the recipe — verified live: ran a real
recipe against a real project in one process while a `--follow` in
another picked up "spec already exists, resuming" → "stage: plan" →
"stage: plan_gate" → two failing gate attempts → a passing one →
"stage: plan_approval" → "PAUSED", all in real time, not replayed after
the fact.

This is deliberately separate from `moor audit`, not a flag on it:
`moor audit` is the full evidence record (raw JSON, every exec's argv
and exit code, egress verdicts, tripwires) for review and export;
`moor logs` is a narrower, human-readable "where's it at right now"
view over one project's recipe activity. Conflating the two would have
made the terse status view the user actually asked for harder to read,
not easier.

## What this deliberately does not do

- **No live streaming of a step's own command output.** A long `keel
  run` build still prints nothing until it finishes — `moor logs` tells
  you *that* it started and which attempt it's on, not a live tail of
  its actual stdout. Getting that would mean switching `exec_capture`
  from `Command::output()` (waits for exit) to a streaming spawn, which
  is a real change to how `moor recipe` inspects command output to
  drive its own logic, not a logging-only change — left for if it's
  ever actually needed.
- **No writing into the container's own log stream.** `docker logs
  <container>` shows only that container's PID 1 output; getting
  recipe events to show up there would mean writing into another
  process's stdout fd from inside a non-root, read-only-rootfs
  container — a fragile trick, not a structural guarantee, and not
  worth it when the audit chain already gives every terminal a durable,
  tamper-evident place to read the same information from.

## Consequences

- `recipe-event` is a new, additive `kind` in the audit chain schema —
  `moor audit`'s existing full-chain view and `--verify` both already
  handle an unrecognized `kind` transparently (verification only checks
  the hash chain's structural integrity, not each kind's meaning), so
  this needed no changes there.
- `moor logs`'s `--follow` is a simple 500ms poll loop over a local
  file, not inotify/fsevents-based — adequate for a chain that gets a
  handful of appends per pipeline stage, not a design for high-frequency
  logging.
