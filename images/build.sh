#!/usr/bin/env bash
# Build (and tag) every moor sandbox image locally.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

echo "==> building moor/base:latest"
docker build -t moor/base:latest ./base

for lang in node rust python; do
  echo "==> building moor/${lang}:latest"
  docker build -t "moor/${lang}:latest" --build-arg BASE_IMAGE=moor/base:latest "./${lang}"
done

echo "==> building moor/egress:latest"
docker build -t moor/egress:latest ../proxy

echo "==> done"
docker images 'moor/*'
