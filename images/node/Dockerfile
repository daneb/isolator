# Node.js project layer. Base image already has a Node runtime (needed for
# the Claude Code CLI); this just adds common package-manager tooling.
ARG BASE_IMAGE=moor/base:latest
FROM ${BASE_IMAGE}

USER root
RUN npm install -g pnpm yarn && npm cache clean --force
USER agent
