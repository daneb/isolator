# Python project layer.
ARG BASE_IMAGE=isolator/base:latest
FROM ${BASE_IMAGE}

USER root
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 python3-venv python3-pip \
    && rm -rf /var/lib/apt/lists/* \
    && ln -sf /usr/bin/python3 /usr/local/bin/python
USER agent

RUN python3 -m pip install --user --break-system-packages --no-cache-dir pipx \
    && python3 -m pipx ensurepath
