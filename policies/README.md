# Hardening policy

The flags every sandbox container is started with (rendered into
`compose/project.compose.yml.tmpl` by the `isolator` CLI):

| Control | Setting | Why |
| --- | --- | --- |
| Filesystem | `read_only: true` + `tmpfs: [/tmp]` + a small named cache volume | The container's own root filesystem cannot be modified at runtime; only `/workspace`, `/tmp`, and the package-cache path are writable. |
| Host mounts | none | See [../docs/THREAT-MODEL.md](../docs/THREAT-MODEL.md) — no bind mount, ever. |
| `docker.sock` | never mounted | A sandbox with the Docker socket can control every other container, including the egress gateway and other projects' sandboxes. This is treated as equivalent to root on the host and is never granted. |
| Capabilities | `cap_drop: [ALL]`, nothing added back | The base image needs no Linux capability beyond what an unprivileged process already has (it never binds a port < 1024, never changes file ownership across users, never mounts anything). |
| Privilege escalation | `security_opt: [no-new-privileges:true]` | Blocks `setuid`/`setgid` binaries (and anything else) from gaining privileges the container didn't start with. |
| User | fixed non-root UID/GID baked into the image (`agent`, 10001:10001) | The container never runs as root, so a capability or seccomp gap is less likely to matter. |
| Resources | `pids_limit`, `mem_limit`, `cpus` from the project manifest | Bounds a runaway build/agent loop; also blunts fork-bomb-style DoS against the host. |
| Network | attached only to the project's own bridge network, routed through the egress gateway | See [../proxy/README.md](../proxy/README.md). No `network_mode: host`, ever. |
| Seccomp | Docker Engine's built-in default profile (not overridden) | A custom, hand-written seccomp allow-list is easy to get subtly wrong (silently breaking a toolchain, or worse, silently allowing something it shouldn't). Docker's shipped default already blocks the highest-risk syscalls (`ptrace`, raw socket creation, kernel module loading, etc.) and is maintained upstream. A tighter, project-specific profile is deferred to Phase 9, to be derived from real `strace` output of an actual toolchain run rather than guessed up front. |

## `isolator selftest`

Rather than trust that the compose template is applied correctly by
inspection alone, `isolator selftest <project>` (Phase 3 stub, filled in
during Phase 7) runs `docker inspect` against the live container and
asserts every row in the table above actually holds — so a future change
to the template that accidentally weakens one of these is caught
immediately, not discovered during an incident.
