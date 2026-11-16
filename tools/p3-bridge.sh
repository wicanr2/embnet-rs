#!/usr/bin/env bash
# P3 的 Bridge 模擬驗證：端到端延遲對照 embnet-config 的上限、Service 跨網段、未列入轉送表與限流、負載轉折點。
#   tools/p3-bridge.sh
set -euo pipefail
cd "$(dirname "$0")/.."
RUN="runs/$(date +%Y%m%d-%H%M%S)-p3-bridge"
mkdir -p "$RUN"
tools/cargo.sh . test --release -p embnet-bridge -- --include-ignored --nocapture --test-threads=1 > "$RUN/sim.log" 2>&1 || true
tools/cargo.sh . test -p embnet-config > "$RUN/config.log" 2>&1 || true
grep -ho 'EMBNET_[A-Z_]* .*' "$RUN/sim.log" | sed "s|\$| run=$RUN|" | tee "$RUN/summary.txt"
F=$( (grep -hc 'test result: FAILED' "$RUN/sim.log" "$RUN/config.log" || true) | awk '{s+=$1} END {print s+0}')
P=$(grep -ho 'test result: ok. [0-9]* passed' "$RUN/sim.log" "$RUN/config.log" | awk '{s+=$4} END {print s+0}')
echo "EMBNET_P3_TESTS passed=$P failed_suites=$F run=$RUN" | tee -a "$RUN/summary.txt"
