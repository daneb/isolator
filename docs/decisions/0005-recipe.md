# ADR-0005: `moor recipe` — a loosely-described outcome drives keel's pipeline

**Status:** Accepted
**Date:** 2026-09-13

## Context

Driving a spec through keel's gated pipeline by hand means typing a long,
exact sequence of commands: `keel spec new`, author the spec content,
`keel gate g0`, `keel approve --stage spec`, `keel plan`, author the
plan/tasks content, `keel gate g1`, `keel approve --stage plan`,
`keel run`, `keel approve --stage merge`. Nothing about that sequence is
hard to automate — keel's own gates are exactly the checkpoints a human
should still decide on — but nobody had written the automation down.

The operator asked for something to "feed to moor" that describes an
outcome *loosely* and drives the rest, stopping for human approval at
the same points keel already defines, then continuing once approved.

## What keel actually gives us (checked live, not assumed)

`keel next --json [slug]` was tried against a real spec, through every
stage, before anything was designed against it:

```json
{"schema":"keel.next/1","blockers":[],"specs":[
  {"slug":"demo-feature","stage":"spec_approval","command":"keel approve demo-feature --stage spec","complete":false}
]}
```

Every stage name seen across a full run: `spec` → `spec_approval` →
`plan` → `plan_gate` → `plan_approval` → `run` → (merge, via G3's
`human-verdict` check, which names `keel approve <slug> --stage merge`
directly in its own failure text — there's no separate `merge_approval`
stage in `keel next`, because G3 already is that checkpoint). Any stage
whose name contains `approval` is unambiguously "a human must act here" —
`command` is always the literal `keel approve ... --stage ...` to run.

## Two problems found only by actually running it, not designed around in advance

1. **A concrete description of the goal makes Claude Code implement it,
   not write a spec about it — even with keel's own authoring prompt
   appended, even with explicit wording asking it not to.** Verified
   directly, three times, while building this feature: given "the
   operator wants a `greet(name)` function," Claude used its own tools to
   write `src/greet.js` (once) and a test file (the next attempt) instead
   of the spec, regardless of how the prompt asked it not to. Fixed
   structurally, not with more careful wording: `claude --print
   --allowedTools Write` for spec-authoring and `--allowedTools Edit` for
   gate-fixing — the same principle as
   [ADR-0003](0003-claude-code-permissions.md), a tool-access boundary
   Claude cannot talk its way around, not a request for restraint.
2. **`--allowedTools <tools...>` is variadic and swallows every argument
   after it until the next flag** — including the prompt itself, if the
   prompt comes after the flag. `claude --print --allowedTools Write
   "<prompt>"` fails with "Input must be provided either through stdin or
   as a prompt argument"; `claude --print "<prompt>" --allowedTools
   Write` works. Found by testing the exact invocation directly, not
   inferred from the `--help` text.
3. **Gate refusals like "already exists" print to stderr; gate pass/fail
   detail prints to stdout.** `cli/src/proc.rs`'s existing `run_capture`
   only captured stdout, so `keel spec new` on an already-scaffolded spec
   (the normal case on every resume after the first run) looked like it
   returned empty output, which the recipe driver misread as a genuine
   failure. Fixed by adding `run_capture_combined` rather than changing
   `run_capture`'s existing behavior for its other callers.

## Decision

`moor recipe <project> <file>` (`cli/src/commands/recipe.rs`). The
recipe file is YAML front matter (`slug`, `scope`, optional `title`,
`max_gate_retries` default 3, `max_run_attempts` default 2) followed by
free text — deliberately not a DSL. The free text is the whole point:
it's handed to the agent close to verbatim, so describing the outcome
loosely is not a workaround, it's the intended way to use this.

The loop, each iteration driven by `keel next --json`:

- No spec yet → `keel spec new` (idempotent — an "already exists" refusal
  is not an error), then author it (`Write`-only agent call).
- `stage` contains `approval` → stop. Print the exact `keel approve ...`
  command and ask the operator to run it, then re-run the recipe.
- `command` is `keel gate ...` → run it; on failure, feed its own output
  to an `Edit`-only agent call and retry, up to `max_gate_retries`.
- `command` is `keel run ...` → run it, regardless of its own exit code
  (a G3 human-verdict block exits non-zero too, and is a legitimate
  pause, not a failure — `keel next` on the next loop is the real source
  of truth). Capped at `max_run_attempts`: verified live that an
  unconfigured or genuinely stuck project (G2.5's `test-movement` heuristic
  blocked, no test file added) will otherwise have `keel next` say
  `stage: run` forever — the cap is load-bearing, not defensive
  boilerplate.
- Anything else (`keel plan ...`, or a future keel stage this driver
  doesn't special-case) → mechanical, just run it.

Every command the recipe issues is logged to the same audit chain as
`moor run`, tagged `kind: "recipe"` instead of `"exec"` so the trail
shows it was recipe-driven.

## What this deliberately does not do

- **No self-correction for the build step itself.** A failing gate on
  spec/plan content (text-only, `Write`/`Edit`-restricted) gets a bounded
  agent-fix loop. A failing `keel run` does not get a "let the agent try
  again automatically" loop beyond the attempt cap — that's real code
  with full tool access, and iterating on it unattended is exactly the
  kind of scope creep this project's whole security posture argues
  against. It stops and hands the evidence to a human.
- **No new approval mechanism.** `keel approve` already is the
  checkpoint; the recipe driver doesn't invent a second one (a lock file,
  a webhook, a poll loop) around it. Re-running the same command is the
  resume mechanism, matching how every other `moor` command already
  works (idempotent, re-invoked by the operator, not a background
  daemon).

## Consequences

- A recipe only drives one spec/slug at a time. Multiple outcomes need
  multiple recipe files, run one at a time — matching keel's own
  documented daily-use assumption (see `policies/README.md`'s note on
  `--waves`: this project's usage is serial by design).
- The stage-name contract (`keel next --json`'s `stage`/`command`/
  `complete` fields, `schema: "keel.next/1"`) is keel's, not moor's —
  if keel changes it, this driver's `match` on `command`'s first two
  words needs revisiting. It fails loudly (prints the raw stage and
  falls through to "run it and see") rather than silently, if that ever
  happens.
- Docker exec's own argv passing (no shell involved) means the recipe's
  free-text description can contain anything — quotes, newlines — with no
  injection risk, the same guarantee `moor run <project> -- <cmd>` already
  has.
