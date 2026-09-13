#!/usr/bin/env sh
# Render the tinyproxy filter file from the baked-in base allow-list plus
# any extra domains the project's moor.yaml `egress.allow` supplied
# via $EXTRA_ALLOW_DOMAINS (newline-separated plain hostnames — turned
# into anchored regex here, the operator never hand-writes regex).
set -eu

mkdir -p /run/tinyproxy
FILTER_FILE=/run/tinyproxy/filter
cp /etc/tinyproxy/allowlist.base.txt "$FILTER_FILE"

if [ -n "${EXTRA_ALLOW_DOMAINS:-}" ]; then
  echo "" >> "$FILTER_FILE"
  echo "# per-project additions (moor.yaml egress.allow)" >> "$FILTER_FILE"
  echo "$EXTRA_ALLOW_DOMAINS" | tr ',' '\n' | while IFS= read -r host; do
    host="$(echo "$host" | tr -d '[:space:]')"
    [ -z "$host" ] && continue
    escaped=$(echo "$host" | sed 's/\./\\./g')
    echo "^([a-zA-Z0-9-]+\\.)*${escaped}\$" >> "$FILTER_FILE"
  done
fi

echo "moor-egress: active allow-list:" >&2
cat "$FILTER_FILE" >&2

exec tinyproxy -d -c /etc/tinyproxy/tinyproxy.conf
