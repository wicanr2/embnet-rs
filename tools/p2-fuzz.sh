#!/usr/bin/env bash
# P2 的 fuzz（docs/40-rust-embedded/03）。每個 target 跑固定秒數，再對語料庫算行覆蓋率。
#   SECONDS_PER_TARGET=300 tools/p2-fuzz.sh [target ...]
set -euo pipefail
cd "$(dirname "$0")/.."
RUN="runs/$(date +%Y%m%d-%H%M%S)-p2-fuzz"
mkdir -p "$RUN"
SECS="${SECONDS_PER_TARGET:-300}"
TARGETS=("$@")
[ ${#TARGETS[@]} -gt 0 ] || TARGETS=(node_on_frame datagram e2e_check config_parse)
IMG=embnet-fuzz:nightly-2026-11-01-r1
dr() { # 在 fuzz 映像裡執行
  docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -e CARGO_HOME=/cargo --cpus 4 --memory 6g --pids-limit 1024 \
    --network "${NET:-none}" --log-opt max-size=10m --log-opt max-file=3 \
    -v embnet-cargo:/cargo -v "$PWD":/w -w /w/fuzz "$IMG" "$@"
}
NET=bridge dr cargo fetch -q > "$RUN/fetch.log" 2>&1
LLVM=$(dr sh -c 'ls -d /usr/local/rustup/toolchains/nightly-*/lib/rustlib/x86_64-unknown-linux-gnu/bin' | head -1)
for t in "${TARGETS[@]}"; do
  mkdir -p "fuzz/corpus/$t"
  dr cargo fuzz run "$t" -- -max_total_time="$SECS" -rss_limit_mb=2048 > "$RUN/$t.run.log" 2>&1 && CRASH=0 || CRASH=1
  N=$(find "fuzz/corpus/$t" -type f | wc -l)
  dr cargo fuzz coverage "$t" > "$RUN/$t.cov.log" 2>&1 || true
  BIN=target/x86_64-unknown-linux-gnu/coverage/x86_64-unknown-linux-gnu/release/$t
  dr "$LLVM/llvm-cov" report "$BIN" -instr-profile="coverage/$t/coverage.profdata" \
     -ignore-filename-regex='(/cargo/registry|/rustc/|fuzz_targets)' > "$RUN/$t.report.txt" 2>&1 || true
  case "$t" in
    datagram) PAT="datagram\\.rs|crc\\.rs" ;;
    e2e_check) PAT="e2e\\.rs" ;;
    node_on_frame) PAT="embnet-node|can/|e2e\\.rs|heartbeat\\.rs" ;;
    config_parse) PAT="embnet-config" ;;
  esac
  dr "$LLVM/llvm-cov" report "$BIN" -instr-profile="coverage/$t/coverage.profdata" -show-functions=false \
     -ignore-filename-regex='(/cargo/registry|/rustc/|fuzz_targets)' $(dr sh -c "find /w/crates -name '*.rs' -path '*/src/*'" | grep -E "$PAT") > "$RUN/$t.scope.txt" 2>&1 || true
  COV=$(awk '$1=="TOTAL"{print $(NF-3)}' "$RUN/$t.scope.txt")
  ALL=$(awk '$1=="TOTAL"{print $(NF-3)}' "$RUN/$t.report.txt")
  HOURS=$(awk -v s="$SECS" 'BEGIN{printf "%.3f", s/3600}')
  STATUS=$([ "$CRASH" = 0 ] && echo PASS || echo FAIL)
  echo "EMBNET_FUZZ $STATUS target=$t hours=$HOURS corpus=$N cov_metric=line cov=${COV:-n/a} cov_scope=$(echo "$PAT" | tr "|" "+" | tr -d "\\\\") cov_crate=${ALL:-n/a} crashes=$CRASH run=$RUN" | tee -a "$RUN/summary.txt"
done
