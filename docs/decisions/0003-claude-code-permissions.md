# ADR-0003: Default Claude Code to `auto` permission mode, not a bypass

**Status:** Accepted
**Date:** 2026-09-13

## Context

Running Claude Code non-interactively — `isolator run <project> -- claude
--print "..."`, the pattern `keel` itself uses to drive an agent, and the
only way to run it through `isolator run` at all (no TTY is attached to
a `docker exec` invocation that runs one command and returns) — hits
Claude Code's permission system with nobody available to answer a
prompt. With no flag, every tool call is silently refused.

The first version of [docs/examples/ascii-banner](../examples/ascii-banner)
used `--dangerously-skip-permissions` to get past this. That's a
blanket bypass — every tool call runs with zero checks, on the
reasoning that the sandbox container is the trust boundary instead
(see `docs/ARCHITECTURE.md`: "keel assumes a trusted host and does no
sandboxing itself; this container *is* that trust boundary"). That
reasoning holds for blast-radius containment, but it throws away a
layer that costs nothing to keep: there's no reason an agent inside the
sandbox should attempt something like `rm -rf /workspace/*` just
because the container would survive it.

Publicly documented incidents where an agent with unrestricted tool
access was manipulated (via prompt injection or a compromised
dependency) into taking actions its operator never asked for are exactly
the risk category `--dangerously-skip-permissions` opts all the way out
of checking for.

## Options considered

### 1. `--dangerously-skip-permissions` (previous behavior, rejected)

Works, but "dangerous" is accurate: zero judgment is applied to any tool
call, so a manipulated or buggy agent run has nothing but the sandbox's
own containment standing between it and whatever it attempts. That
containment is real and substantial — but it's meant to be the second
line, not the only one.

### 2. A curated `--allowedTools`/`--disallowedTools` list, isolator-maintained (rejected)

Considered and rejected: a hand-written allow/deny list would need
isolator to track Claude Code's tool surface indefinitely and would
rot every time that surface changes — exactly the kind of
configuration-based control this project has otherwise avoided in favor
of structural ones (see the README's Security section: "the core
guarantee is structural, not configurable").

### 3. Claude Code's own `auto` permission mode (chosen)

`--permission-mode auto`, or equivalently `{"permissions": {"defaultMode":
"auto"}}` in `~/.claude/settings.json` — Claude Code's own current
default permission mode outside of headless-with-a-flag contexts.
Verified directly, not assumed: inside a live sandbox container, with no
flag at all beyond the settings file, it created a file when asked, and
refused `rm -rf /workspace/*` unprompted, describing exactly why. It
runs non-interactively (no TTY needed) and requires no isolator-specific
tool-list maintenance — the risk judgment is Anthropic's to keep current
with the tool surface, not isolator's.

## Decision

`images/base/claude-settings.json` (`{"permissions": {"defaultMode":
"auto"}}`) is copied to `/home/agent/.claude/settings.json` in
`images/base/Dockerfile`, so every project gets this as its default
without any per-invocation flag. `--dangerously-skip-permissions` is no
longer isolator's documented or demonstrated pattern anywhere.

## Consequences

- Two independent layers now stand between a manipulated or buggy agent
  run and real damage: Claude's own per-call risk assessment (auto
  mode), and the sandbox's structural containment (no bind mount,
  `cap_drop: ALL`, non-root, default-deny egress) if the first layer is
  ever fooled.
- An operator who genuinely wants the old blanket-bypass behavior for a
  specific run can still pass `--dangerously-skip-permissions`
  explicitly via `isolator run <project> -- claude --print
  --dangerously-skip-permissions "..."` — this changes the *default*,
  not what's possible.
- Auto mode's risk judgment is a model behavior Anthropic maintains and
  can change between Claude Code versions; isolator doesn't control or
  pin that logic, only the default that selects it.
- `docs/examples/ascii-banner` was rebuilt and re-verified under this
  default (no bypass flag) to keep that example honest against the
  actual current recommendation, not just this ADR's text.
