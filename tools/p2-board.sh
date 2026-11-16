#!/usr/bin/env bash
# 實板初測（NUCLEO-F767ZI，ST-LINK/V2.1）：燒錄、重設、跑 3 秒後用 probe-rs 讀出結果。
#   tools/p2-board.sh [cycles|can-order ...]
# 需要主機的 udev 規則讓 plugdev 群組讀寫 ST-LINK（見 docs/plans/007）。
set -euo pipefail
cd "$(dirname "$0")/.."
RUN="runs/$(date +%Y%m%d-%H%M%S)-p2-board"
mkdir -p "$RUN"
CHIP=STM32F767ZITx
BINS=("$@")
[ ${#BINS[@]} -gt 0 ] || BINS=(cycles can-order)

DEV=$(lsusb | awk '/0483:374b/{print "/dev/bus/usb/"$2"/"substr($4,1,3); exit}')
[ -n "$DEV" ] || { echo "找不到 ST-LINK/V2.1" >&2; exit 1; }
[ -w "$DEV" ] || { echo "$DEV 沒有寫入權限，請先安裝 udev 規則" >&2; exit 1; }
PLUGDEV=$(getent group plugdev | cut -d: -f3)

tools/cargo.sh probes/p2-board build --release > "$RUN/build.log" 2>&1
probe() {
  timeout 120 docker run --rm --network none --user "$(id -u):$(id -g)" --group-add "$PLUGDEV" -e HOME=/tmp \
    --cpus 1 --memory 512m --pids-limit 64 --log-opt max-size=10m --log-opt max-file=3 \
    --device "$DEV" -v "$PWD/probes/p2-board/target":/t:ro embnet-probe:0.32.0-r1 "$@"
}
nm_addr() {
  docker run --rm --network none --user "$(id -u):$(id -g)" --log-opt max-size=10m --log-opt max-file=3 \
    -v "$PWD/probes/p2-board/target":/t:ro embnet-rust:1.100.0-r1 arm-none-eabi-nm "/t/thumbv7em-none-eabihf/release/$1" \
    | awk -v s="$2" '$3==s{print "0x"$1}'
}

for b in "${BINS[@]}"; do
  case "$b" in
    cycles) SYM=P1_RESULT ;;
    can-order) SYM=P2_RESULT ;;
    *) echo "未知的韌體 $b" >&2; exit 2 ;;
  esac
  ADDR=$(nm_addr "$b" "$SYM")
  probe sh -c "probe-rs download --chip $CHIP /t/thumbv7em-none-eabihf/release/$b && probe-rs reset --chip $CHIP && sleep 3 && probe-rs read --chip $CHIP b32 $ADDR 16" > "$RUN/$b.log" 2>&1
  tail -2 "$RUN/$b.log" > /dev/null
done

# 解析成通過行
words() { grep -E '^[0-9a-f]{8}:' "$1" | cut -d: -f2 | tr ' ' '\n' | grep -E '^[0-9a-f]{8}$'; }
if [ -f "$RUN/cycles.log" ]; then
  mapfile -t W < <(words "$RUN/cycles.log")
  if [ "${W[0]:-}" = 0c1c0c1c ]; then
    HZ=$((16#${W[14]}))
    NAMES=(mac32_gen mac32_verify e2e32_gen e2e32_verify canid_decode segment_9B reassemble_2f datagram_decode_20B mac32_gen_cached mac32_verify_cached mac_key_expand)
    line="EMBNET_BOARD_COST board=nucleo-f767zi sysclk_hz=$HZ cache=on art=on unit=cycles k=$((16#${W[1]}))"
    for i in "${!NAMES[@]}"; do line="$line ${NAMES[$i]}=$((16#${W[$((i + 2))]}))"; done
    echo "$line run=$RUN" | tee -a "$RUN/summary.txt"
  else
    echo "EMBNET_BOARD_COST FAIL marker=${W[0]:-none} run=$RUN" | tee -a "$RUN/summary.txt"
  fi
fi
if [ -f "$RUN/can-order.log" ]; then
  mapfile -t W < <(words "$RUN/can-order.log")
  h() { printf '0x%X' $((16#$1)); }
  ord() { echo "$(h "$1"),$(h "$2"),$(h "$3")"; }
  if [ "${W[0]:-}" = 0ca00ca0 ]; then
    a=$(ord "${W[1]}" "${W[2]}" "${W[3]}"); b=$(ord "${W[4]}" "${W[5]}" "${W[6]}")
    c=$(ord "${W[7]}" "${W[8]}" "${W[9]}"); d=$(ord "${W[10]}" "${W[11]}" "${W[12]}")
    ok=PASS
    [ "$a" = "0x100,0x200,0x300" ] && [ "$b" = "0x300,0x100,0x200" ] && [ "$c" = "0x300,0x100,0x200" ] && [ "$d" = "0x300,0x200,0x100" ] || ok=FAIL
    echo "EMBNET_BOARD_CAN_ORDER $ok board=nucleo-f767zi can=CAN1 mode=loopback-silent prefill_txfp0=$a prefill_txfp1=$b live_txfp0=$c live_txfp1=$d put_fail=$((16#${W[13]})) run=$RUN" | tee -a "$RUN/summary.txt"
  else
    echo "EMBNET_BOARD_CAN_ORDER FAIL marker=${W[0]:-none} progress=${W[14]:-none} run=$RUN" | tee -a "$RUN/summary.txt"
  fi
fi
