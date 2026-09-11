# Rust project layer.
ARG BASE_IMAGE=isolator/base:latest
FROM ${BASE_IMAGE}

USER root
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
USER agent

ENV RUSTUP_HOME=/home/agent/.rustup \
    CARGO_HOME=/home/agent/.cargo \
    PATH=/home/agent/.cargo/bin:$PATH

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain stable
