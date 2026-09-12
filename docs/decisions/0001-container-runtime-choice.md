# ADR-0001: Container runtime stays `runc` via OrbStack/Docker

**Status:** Accepted
**Date:** 2026-09-12

## Context

isolator's entire threat model rests on one property: a sandboxed
container can never reach the host OS, and its only route to the
internet is through an explicit, auditable allow-list. Two alternative
runtimes were investigated as ways to strengthen that further —
stronger syscall isolation (gVisor) or true per-container VM isolation
(Apple's native `container` tool) — before committing to `runc` as
implemented.

## Options considered

### 1. gVisor (`runsc`) under the current Docker/OrbStack setup

Rejected — not achievable on this platform, not a product choice.
Checked directly: `docker info` on this OrbStack install reports only
`runc`/`io.containerd.runc.v2`. Switching to Docker Desktop wouldn't
help — both Mac Docker products run their engine inside a managed VM
with no supported way to register a custom OCI runtime in its
containerd config. `orb`'s shell/machine access (`orb create ubuntu`)
is a separate general-purpose-VM feature unrelated to the VM actually
running the Docker engine. Real gVisor needs a Linux host or
self-managed Linux VM with containerd configured directly — a bigger
architecture change (a remote `DOCKER_HOST`), not a runtime flag.

### 2. Apple's native `container` (Containerization framework)

Rejected for now — not a maturity judgment, a confirmed security gap.
Structurally the most interesting option: each container gets its own
lightweight VM via `Virtualization.framework`, so there is no shared
kernel across containers at all (Firecracker/microVM-style), which
would close the "kernel escape" concern more fundamentally than gVisor
does. But:

- Requires macOS 26 (confirmed against
  [github.com/apple/container](https://github.com/apple/container));
  this machine runs 15.3.1. Apple's own maintainers state they
  typically won't address issues that can't be reproduced on macOS 26.
- No compose-equivalent exists — adopting it means rewriting
  isolator's entire `compose/` orchestration layer, not just swapping
  a binary.
- **The disqualifying finding:** default-deny network egress — the
  single control isolator depends on most — is not solved. In
  [apple/container#719](https://github.com/apple/container/discussions/719),
  a user attempted almost exactly isolator's dual-homed egress-proxy
  pattern (an agent container on an internal network, a proxy
  container filtering what it can reach) and found that **the host
  gateway remains reachable from the "internal" network, so any host
  service bound to `0.0.0.0` is reachable from inside the sandboxed VM
  without going through the proxy at all.** That is a direct hole in
  "never touch the host OS," still open as of this check. The only
  workaround discussed is hand-written host-side macOS `pf` firewall
  rules — global, mutable, host-level state outside the project, which
  `isolator selftest` has no way to verify the way it verifies the
  current compose-based controls.

### 3. `runc` via Docker/OrbStack, as already implemented (chosen)

Every control isolator's threat model requires is already implemented
and verified against a live container by `isolator selftest`: no host
bind mount, no `docker.sock`, `cap_drop: ALL`, `no-new-privileges`,
non-root user, a genuinely internal Docker network (`internal: true`)
with no route to the internet except through the egress gateway
container. This was proven, not assumed — see
[docs/WALKTHROUGH.md](../WALKTHROUGH.md) for a real project run
through the whole pipeline, and `tests/e2e.sh` for the automated
version including a live tamper attempt against the audit chain.

## Decision

Stay on `runc` via the current Docker/OrbStack setup. Neither
alternative investigated is currently reachable (gVisor) or safe for
this threat model (Apple's `container`, today).

## Consequences

- The "shared kernel" risk category (a `runc`/kernel syscall escape)
  remains open, mitigated only by staying current on Docker/OrbStack
  updates. This is a narrower residual risk than what's already closed
  by the no-bind-mount / no-docker.sock / default-deny-egress controls.
- Revisit Apple's `container` only if
  [apple/container#719](https://github.com/apple/container/discussions/719)
  closes with a real fix for the host-gateway leak — not merely
  because this Mac is upgraded past macOS 26. An OS upgrade removes
  one blocker (the version requirement) but not the disqualifying one
  (the egress hole).
- Revisit gVisor only if a concrete need justifies standing up a
  self-managed Linux host/VM and pointing `isolator` at it remotely —
  not attempted speculatively.
