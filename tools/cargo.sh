#!/usr/bin/env bash
# 在 embnet-rust 容器裡跑 cargo。crate 快取在 docker volume embnet-cargo。
#   tools/cargo.sh <子目錄> <cargo 參數...>          預設 --network none
#   NET=bridge tools/cargo.sh <子目錄> fetch         需要抓 crate 時才開網路
set -euo pipefail
cd "$(dirname "$0")/.."
SUB="$1"; shift
test -d "$SUB" || { echo "no such dir: $SUB" >&2; exit 2; }
exec docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -e CARGO_HOME=/cargo \
  --cpus="${CPUS:-4}" --memory 4g --pids-limit 1024 --network "${NET:-none}" \
  --log-opt max-size=10m --log-opt max-file=3 \
  -v embnet-cargo:/cargo -v "$PWD":/w -w "/w/$SUB" "${IMAGE:-embnet-rust:1.100.0-r1}" cargo "$@"
