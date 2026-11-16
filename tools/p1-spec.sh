#!/usr/bin/env bash
# P1 的驗證：重新產生 test vector、確認與版控中的一致，再用 Rust 實作逐一比對。
#   tools/p1-spec.sh
set -euo pipefail
cd "$(dirname "$0")/.."
RUN="runs/$(date +%Y%m%d-%H%M%S)-p1-spec"
mkdir -p "$RUN"

tools/vectors.sh | tee "$RUN/vectors.log"
if ! git diff --quiet -- docs/spec/vectors; then
  echo "test vector 與版控中的不一致；請檢查 tools/vectors/gen.py 的變更" >&2
  git diff --stat -- docs/spec/vectors >&2
  exit 1
fi
CASES=$(grep -o 'cases=[0-9]*' "$RUN/vectors.log" | cut -d= -f2)

tools/cargo.sh . test -p embnet-core -- --nocapture > "$RUN/test.log" 2>&1
PASSED=$(grep -o 'test result: ok. [0-9]* passed' "$RUN/test.log" | awk '{s+=$4} END {print s+0}')
FAILED=$(grep -o '[0-9]* failed' "$RUN/test.log" | awk '{s+=$1} END {print s+0}')
tools/cargo.sh . clippy -p embnet-core --all-targets > "$RUN/clippy.log" 2>&1
LINT=$(grep -cE '^(error|warning)' "$RUN/clippy.log" || true)
tools/cargo.sh . build -p embnet-core --release --target thumbv7em-none-eabihf > "$RUN/thumb.log" 2>&1 && THUMB=ok || THUMB=fail

STATUS=PASS
[ "$FAILED" = 0 ] && [ "$LINT" = 0 ] && [ "$THUMB" = ok ] || STATUS=FAIL
echo "EMBNET_SPEC $STATUS version=0.1 vectors=$CASES rust_tests=$PASSED failed=$FAILED clippy=$LINT thumbv7em=$THUMB run=$RUN" | tee "$RUN/summary.txt"
