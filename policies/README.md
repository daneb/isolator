# Hardening policy

The flags every sandbox container is started with (rendered into
`compose/project.compose.yml.tmpl` by the `isolator` CLI):

| Control | Setting | Why |
| --- | --- | --- |
| Filesystem | `read_only: true` + `tmpfs: [/tmp]` + small named cache/state volumes | The container's own root filesystem cannot be modified at runtime; only `/workspace`, `/tmp`, the package-cache path, and Claude Code's own state directory (`/home/agent/.claude`, needed for its Bash tool's bookkeeping) are writable — each a named Docker volume, never a host path. |
| Host mounts | none | See [../docs/THREAT-MODEL.md](../docs/THREAT-MODEL.md) — no bind mount, ever. |
| `docker.sock` | never mounted | A sandbox with the Docker socket can control every other container, including the egress gateway and other projects' sandboxes. This is treated as equivalent to root on the host and is never granted. |
| Capabilities | `cap_drop: [ALL]`, nothing added back | The base image needs no Linux capability beyond what an unprivileged process already has (it never binds a port < 1024, never changes file ownership across users, never mounts anything). |
| Privilege escalation | `security_opt: [no-new-privileges:true]` | Blocks `setuid`/`setgid` binaries (and anything else) from gaining privileges the container didn't start with. |
| User | fixed non-root UID/GID baked into the image (`agent`, 10001:10001) | The container never runs as root, so a capability or seccomp gap is less likely to matter. |
| Resources | `pids_limit`, `mem_limit`, `cpus` from the project manifest | Bounds a runaway build/agent loop; also blunts fork-bomb-style DoS against the host. |
| Network | attached only to the project's own bridge network, routed through the egress gateway | See [../proxy/README.md](../proxy/README.md). No `network_mode: host`, ever. |
| Seccomp | Docker Engine's built-in default profile (not overridden) | A custom, hand-written seccomp allow-list is easy to get subtly wrong (silently breaking a toolchain, or worse, silently allowing something it shouldn't). Docker's shipped default already blocks the highest-risk syscalls (`ptrace`, raw socket creation, kernel module loading, etc.) and is maintained upstream. A tighter, project-specific profile is deferred, to be derived from real `strace` output of an actual toolchain run rather than guessed up front. |
| Runtime | `runc` (Docker/OrbStack's default) | Checked directly: `docker info` on this OrbStack install reports only `runc`/`io.containerd.runc.v2`, and `runsc` (gVisor) isn't installed. This is not fixable by switching to Docker Desktop instead — checked that too, structurally: `orb`'s shell/machine access (`orb create ubuntu`) is a separate general-purpose-VM feature unrelated to the VM actually running the Docker engine, and Docker Desktop's own engine likewise runs inside a managed LinuxKit VM with no supported way to register a custom OCI runtime in its containerd config. Both Mac Docker products put the daemon in a VM you don't get to administer at that level — gVisor genuinely needs a Linux host you install and configure containerd on yourself (a real Linux box, or a self-managed Linux VM run *inside* one of `orb create`'s machines, with `isolator` pointed at it remotely via `DOCKER_HOST`). Deferred; only worth building if a concrete need shows up, since the existing controls (no bind mount, no `docker.sock`, cap-drop ALL, default-deny network) already cover the higher-value part of this threat model — gVisor would close the narrower remaining gap of "a runc/kernel syscall escape," not the "container reaches the host" case. |
| Runtime, alternative | Apple's native `container` (Containerization framework) | Investigated in depth, **not viable today** — this isn't "immature, be cautious," it's a confirmed, currently-open gap in exactly the control isolator depends on most. Structurally interesting in principle: each container gets its own lightweight VM via `Virtualization.framework` (no shared kernel across containers — Firecracker/microVM-style), sidestepping the "shared kernel escape" concern more fundamentally than gVisor does. Requires **macOS 26** (confirmed against github.com/apple/container; this Mac is on 15.3.1) and Apple Silicon (already satisfied). But: someone attempted almost exactly isolator's dual-homed egress-proxy pattern on it and found, reported in [apple/container#719](https://github.com/apple/container/discussions/719), that **the host gateway remains reachable from the "internal" network — any host service bound to `0.0.0.0` is reachable from inside the sandbox without going through the proxy at all.** That's a direct hole in "never touch the host OS," not a rough edge, and it's still open as of this check; the only workaround floated is hand-written host-side `pf` firewall rules — global mutable state outside the project, not something `isolator selftest` could verify the way it verifies the current compose-based controls. There's also no compose-equivalent, so the whole `compose/` orchestration layer would need a rewrite regardless. Revisit only if that discussion closes with a real fix — not just because a macOS upgrade removes the version blocker. |

## `isolator selftest`

`isolator selftest <project>` runs two layers of checks against a live
container:

1. **Static checks** (`docker inspect`): asserts every row in the table
   above actually holds — read-only rootfs, dropped capabilities,
   no-new-privileges, not privileged, no bind mounts, non-root user, not
   on the default bridge network, a pids limit. Every default in this
   evaluator leans toward FAIL on missing/malformed data, not PASS — a
   corrupted or truncated `docker inspect` result must never read as "all
   clear" (see the `missing_fields_default_to_failing_safe` test in
   `cli/src/commands/selftest.rs`).
2. **Active breakout battery** (live probes inside the container): tries
   to reach the reserved canary domain (must fail), tries to write to the
   read-only root filesystem (must fail), checks `docker.sock` is not
   present (must be absent). Each run leaves a `tripwire-check` entry in
   the project's audit chain, and folds in any resulting egress-log
   entries — so a canary-domain attempt shows up as a `TRIPWIRE` line in
   `isolator audit`, not just a console message that scrolls away.

So a future change to the compose template or the images that
accidentally weakens one of these is caught immediately, not discovered
during an incident. See `tests/e2e.sh` for the full battery run against a
real container, including a live tamper attempt against the audit chain
itself.
