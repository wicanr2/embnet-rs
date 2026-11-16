#!/usr/bin/env bash
# 實板 loopback 的單匯流排與 Bridge 單跳延遲（probes/p2-board/src/bin/loop-hop.rs）。
#   tools/p3-board.sh
set -euo pipefail
cd "$(dirname "$0")/.."
RUN="runs/$(date +%Y%m%d-%H%M%S)-p3-board"
mkdir -p "$RUN"
CHIP=STM32F767ZITx
ELF=thumbv7em-none-eabihf/release/loop-hop
DEV=$(lsusb | awk '/0483:374b/{print "/dev/bus/usb/"$2"/"substr($4,1,3); exit}')
[ -w "$DEV" ] || { echo "ST-LINK 沒有寫入權限" >&2; exit 1; }
PLUGDEV=$(getent group plugdev | cut -d: -f3)
tools/cargo.sh probes/p2-board build --release --bin loop-hop > "$RUN/build.log" 2>&1
addr() {
  docker run --rm --network none --user "$(id -u):$(id -g)" --log-opt max-size=10m --log-opt max-file=3 \
    -v "$PWD/probes/p2-board/target":/t:ro embnet-rust:1.100.0-r1 arm-none-eabi-nm "/t/$ELF" | awk -v s="$1" '$3==s{print "0x"$1}'
}
R=$(addr P3_RESULT); A=$(addr HIST_A); B=$(addr HIST_B)
timeout 180 docker run --rm --network none --user "$(id -u):$(id -g)" --group-add "$PLUGDEV" -e HOME=/tmp \
  --cpus 1 --memory 512m --pids-limit 64 --log-opt max-size=10m --log-opt max-file=3 \
  --device "$DEV" -v "$PWD/probes/p2-board/target":/t:ro embnet-probe:0.32.0-r1 sh -c "
    probe-rs download --chip $CHIP /t/$ELF >/dev/null 2>&1 && probe-rs reset --chip $CHIP && sleep 48 &&
    echo RESULT && probe-rs read --chip $CHIP b32 $R 16 &&
    echo HIST_A && probe-rs read --chip $CHIP b32 $A 1024 &&
    echo HIST_B && probe-rs read --chip $CHIP b32 $B 1024" > "$RUN/read.log" 2>&1
docker run --rm --network none --user "$(id -u):$(id -g)" --log-opt max-size=10m --log-opt max-file=3 \
  -v "$PWD/$RUN":/r -v "$PWD/tools":/tools:ro embnet-py:3.13-r1 python -I /tools/p3_board_parse.py /r "$RUN" | tee "$RUN/summary.txt"
