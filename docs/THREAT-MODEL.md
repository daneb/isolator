# Threat model

## What moor is protecting

An LLM coding agent (Claude Code today; others may be added later) runs
inside a container, driven by [keel](https://github.com/daneb/keel). keel
has no sandboxing of its own — it trusts the host completely and will
execute any shell command a repo's `.keel/keel.toml` or the agent itself
asks it to. moor's job is to make that assumption safe by making sure
"the host" the agent sees is never the real Mac Mini.

## Assets

- The host OS, its filesystem, its other repos and credentials, and
  anything else running on the Mac Mini (browser sessions, keychain,
  other containers, the network the Mac is on).
- Secrets a project's sandbox legitimately needs (a GitHub token, an
  Anthropic API key, package registry tokens).
- The integrity of the audit trail itself — a compromised sandbox must not
  be able to rewrite or silence its own audit log.
- Other moor projects — a compromised sandbox for project A must not
  reach project B's volume, network, or secrets.

## Trust boundaries

```
Untrusted  →  cloned repo content, `.keel/keel.toml`, agent output, anything
              the agent writes to disk or emits on stdout
Sandboxed  →  the container: agent CLI, keel, the toolchain, the workspace
              volume, the sandbox's own process tree
Trusted    →  the host: the moor CLI process, ~/.moor/, Docker
              Engine itself, the egress proxy's allow-list source of truth
```

A cloned repository is untrusted input the moment it exists inside the
sandbox — same status as the SECURITY.md warning in keel itself. moor
does not try to make repo content trustworthy; it makes the blast radius
of trusting it equal to "one throwaway container," never "the Mac Mini."

## What counts as a breakout (v1 definitions)

1. **Filesystem breakout** — any process inside the sandbox reads or
   writes a path outside its own container's namespace: the host
   filesystem, another project's volume, or the Docker socket. Prevented
   structurally: no bind mounts, `docker.sock` never mounted, no shared
   volumes between projects, read-only rootfs.
2. **Network breakout** — any outbound connection that does not go through
   the project's egress proxy, or that the proxy allows but shouldn't
   (a domain not on `moor.yaml`'s `egress.allow` list). Prevented by
   the custom bridge network having no other route to the internet, and
   detected by the proxy's own access log plus `moor selftest`.
3. **Secret exfiltration** — a secret value (API key, GitHub token)
   leaving the sandbox by any channel other than its intended use (e.g.
   TLS to `api.anthropic.com`, or `git push` to `github.com`). Detected by
   the canary-token watch in the egress proxy: a decoy secret is planted
   alongside real ones, and the proxy flags any request — allowed
   destination or not — whose payload contains that literal string.
4. **Privilege escalation inside the container** — the agent process
   gaining root or new Linux capabilities it didn't start with. Prevented
   by `cap_drop: [ALL]`, `no-new-privileges`, non-root UID from container
   start, default seccomp profile.
5. **Audit tampering** — the sandbox writing to or deleting
   `~/.moor/projects/<name>/audit/*`. Prevented structurally: that
   directory lives on the host and is never mounted into any container;
   everything written there is written by the host-side `moor`
   process itself, not by the container. The audit trail itself
   (`audit/chain.jsonl`) is additionally a SHA-256 hash chain — each entry
   commits to the hash of the entry before it, so even someone with
   direct filesystem access to `~/.moor/` (not the sandbox — the
   *operator's own machine*) can't edit, delete, reorder, or insert an
   entry without `moor audit <name> --verify` detecting exactly where
   the chain breaks. This defends against accidental corruption and
   against a compromised process on the host with file access but not
   the ability to observe every command as it ran; it does not defend
   against someone who controls the `moor` binary itself (see
   "explicitly out of scope" below).

## Why not a git pre-push hook for detecting code leaving the sandbox

An earlier version of this plan considered baking a `pre-push` git hook
into the base image to log every push. It's not used: a git hook runs
*inside* the sandbox, under the same account that controls its own git
config — the agent (or a cloned repo's `.gitattributes`/`core.hooksPath`
override) can simply reconfigure or bypass it, so it would be a control
that looks like a boundary but isn't one. Instead:

- `moor run <project> -- git push ...` is tagged `kind: "git-push"` in
  the audit chain by the CLI itself (host-side, so it's a real signal,
  not something the sandbox can suppress) — but it only sees commands run
  through `moor run`, not ones typed inside an interactive `moor
  shell` session.
- The **egress gateway's own log**, folded into the audit chain by
  `moor audit`, is the reliable signal: every connection to
  `github.com` is recorded from *outside* the sandbox, and nothing inside
  the sandbox can prevent or edit that record. It can't distinguish a
  push from a fetch/clone (TLS isn't intercepted — see
  proxy/README.md) but it can't be lied to about whether the connection
  happened at all.

## Explicitly out of scope for v1

- Kernel-level container-escape 0-days (runc/containerd vulnerabilities).
  Mitigated only by staying current on Docker/OrbStack updates; a stronger
  runtime (gVisor) is deferred to Phase 9.
- Supply-chain compromise of the base image's own dependencies (a
  malicious `git`, `curl`, or `cargo install keel-harness` payload at
  image build time). Mitigated by pinning versions and building images on
  the host, not letting a sandbox rebuild its own image.
- Physical access to the Mac Mini.
- A malicious *moor* CLI binary itself — the CLI is trusted; if it's
  compromised, the whole model fails. Treat `cli/` changes with the same
  scrutiny as anything else that runs unsandboxed on the host.
- Multi-tenant use (multiple distinct humans sharing one moor
  install). v1 assumes a single operator.

## Why containers, not a VM per project

A full VM per project would be stronger isolation but far more friction
for "ideation to shipped in one sitting." Docker's namespace/cgroup
isolation, combined with no bind mounts, no host network mode, and a
locked-down egress path, is judged sufficient for the threat model above —
an agent that misbehaves inside its container loses at worst that one
container's volume and whatever the egress proxy let it reach, never the
host. If that judgment turns out wrong in practice (a real breakout is
observed), the response is Phase 9's stronger runtime, not abandoning the
container model.
