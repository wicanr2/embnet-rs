#!/usr/bin/env bash
# 在 embnet-py 容器裡重新產生 docs/spec/vectors/。
set -euo pipefail
cd "$(dirname "$0")/.."
docker run --rm --network none --user "$(id -u):$(id -g)" --cpus 1 --memory 512m --pids-limit 64 \
  --log-opt max-size=10m --log-opt max-file=3 \
  -v "$PWD/tools/vectors":/gen:ro -v "$PWD/docs/spec":/spec embnet-py:3.13-r1 \
  python -I /gen/gen.py /spec/vectors
