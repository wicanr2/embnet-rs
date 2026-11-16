#!/usr/bin/env bash
# P2 的單一匯流排驗證：模擬匯流排上的功能與故障注入（EMBNET_FAULT），以及主機上的處理時間分布（EMBNET_BUS）。
#   tools/p2-bus.sh
# 處理時間會受同時執行的其他工作影響，量測時不要和 fuzz 一起跑。
set -euo pipefail
cd "$(dirname "$0")/.."
RUN="runs/$(date +%Y%m%d-%H%M%S)-p2-bus"
mkdir -p "$RUN"
tools/cargo.sh . test -p embnet-node --test sim -- --nocapture --test-threads=1 > "$RUN/sim.log" 2>&1 || true
tools/cargo.sh . test --release -p embnet-node --test bus -- --ignored --nocapture > "$RUN/bus.log" 2>&1 || true
grep -ho 'EMBNET_[A-Z_]* .*' "$RUN/sim.log" "$RUN/bus.log" | sed "s|\$| run=$RUN|" | tee "$RUN/summary.txt"
FAILED=$( (grep -hc 'test result: FAILED' "$RUN/sim.log" "$RUN/bus.log" || true) | awk '{s+=$1} END {print s+0}')
PASSED=$(grep -ho 'test result: ok. [0-9]* passed' "$RUN/sim.log" "$RUN/bus.log" | awk '{s+=$4} END {print s+0}')
echo "EMBNET_P2_TESTS passed=$PASSED failed_suites=$FAILED run=$RUN" | tee -a "$RUN/summary.txt"
