# isolator base sandbox image.
# Hardened, non-root, no host tooling beyond what an agent needs to build/
# lint/test a repo under keel's direction. No docker CLI, no sudo, no SSH
# server, no bind-mount assumptions anywhere.

# --- stage 1: build keel from crates.io -------------------------------
FROM rust:1-slim-bookworm AS keel-build
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install keel-harness --locked

# --- stage 2: runtime image ---------------------------------------------
FROM debian:bookworm-slim AS runtime

ARG AGENT_UID=10001
ARG AGENT_GID=10001

RUN apt-get update && apt-get install -y --no-install-recommends \
    git curl jq ripgrep ca-certificates gnupg \
    && curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# Claude Code CLI. No auth material baked in — an API key/token is
# injected as an env var at `isolator up`, never stored in the image.
RUN npm install -g @anthropic-ai/claude-code && npm cache clean --force

COPY --from=keel-build /usr/local/cargo/bin/keel /usr/local/bin/keel

RUN groupadd -g "${AGENT_GID}" agent \
    && useradd -m -u "${AGENT_UID}" -g "${AGENT_GID}" -s /bin/bash agent \
    && mkdir -p /workspace /home/agent/.cache /home/agent/.claude \
    && chown -R agent:agent /workspace /home/agent/.cache /home/agent/.claude

# /home/agent/.cache and /home/agent/.claude must exist with agent
# ownership *in the image*, not just at container runtime: isolator mounts
# named volumes there (never a host path — see docs/ARCHITECTURE.md), and
# Docker only copies a fresh volume's initial ownership from whatever
# already exists at that path in the image. Skip this and the mount point
# gets created root-owned instead, silently breaking every Bash tool call
# Claude Code makes (it needs to write to /home/agent/.claude) under the
# read-only, non-root, cap-drop-ALL rootfs below.

WORKDIR /workspace
USER agent

# No ENTRYPOINT/CMD that assumes a shell — isolator drives every
# invocation explicitly (`docker exec`), so the container's default
# command just idles.
CMD ["sleep", "infinity"]
