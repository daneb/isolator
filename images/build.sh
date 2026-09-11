#!/usr/bin/env bash
# Build (and tag) every isolator sandbox image locally.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

echo "==> building isolator/base:latest"
docker build -t isolator/base:latest ./base

for lang in node rust python; do
  echo "==> building isolator/${lang}:latest"
  docker build -t "isolator/${lang}:latest" --build-arg BASE_IMAGE=isolator/base:latest "./${lang}"
done

echo "==> building isolator/egress:latest"
docker build -t isolator/egress:latest ../proxy

echo "==> done"
docker images 'isolator/*'
