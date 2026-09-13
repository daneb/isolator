# moor egress gateway: a default-deny forward proxy that is a
# project sandbox's only route to the internet.
FROM debian:bookworm-slim

# `upgrade`, not just `install` — see images/base/Dockerfile for why.
RUN apt-get update && apt-get upgrade -y && apt-get install -y --no-install-recommends \
    tinyproxy \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /var/log/tinyproxy && chown tinyproxy:tinyproxy /var/log/tinyproxy

COPY tinyproxy.conf.template /etc/tinyproxy/tinyproxy.conf
COPY allowlist.base.txt /etc/tinyproxy/allowlist.base.txt
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

# Run as the tinyproxy user from the start — see the note in
# tinyproxy.conf.template about why this matters under cap_drop: ALL.
USER tinyproxy

EXPOSE 8888
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
