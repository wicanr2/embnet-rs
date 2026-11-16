#!/usr/bin/env bash
# P0 的 Renode 檢查（docs/plans/004 第六節）。
#   tools/p0-renode.sh
# 韌體把結果寫進 P0_RESULT，monitor 腳本跑 2 秒虛擬時間後讀出；DWT CYCCNT 與 TIM2 CNT 直接從 monitor 讀。
set -euo pipefail
cd "$(dirname "$0")/.."
RUN="runs/$(date +%Y%m%d-%H%M%S)-p0-renode"
mkdir -p "$RUN"
IMG=embnet-renode:1.17.0-r1
ELF=target/thumbv7em-none-eabihf/release/p0-renode

tools/cargo.sh probes/p0-renode build --release > "$RUN/build.log" 2>&1
ADDR=$(docker run --rm --network none --user "$(id -u):$(id -g)" --log-opt max-size=10m --log-opt max-file=3 \
  -v "$PWD/probes/p0-renode":/w:ro embnet-rust:1.100.0-r1 arm-none-eabi-nm "/w/$ELF" | awk '$3=="P0_RESULT"{print $1}')
test "$ADDR" = "20000058" || { echo "P0_RESULT 位址改變：$ADDR，請更新 renode/p0.resc" >&2; exit 1; }

timeout 300 docker run --rm --network none --user "$(id -u):$(id -g)" -e HOME=/tmp --cpus 2 --memory 2g --pids-limit 512 \
  --log-opt max-size=10m --log-opt max-file=3 -v "$PWD/probes/p0-renode":/w:ro "$IMG" \
  renode --disable-xwt --console --plain -e "\$elf=@/w/$ELF; include @/w/renode/p0.resc" 2>&1 | tr -d '\r' > "$RUN/renode.log"

vals=$(sed -n '/P0_RESULT_BEGIN/,/P0_RESULT_END/p' "$RUN/renode.log" | grep '^0x')
read -r r0 r1 r2 r3 _ _ <<<"$(echo $vals)"
cyc=$(grep -A1 '^DWT_CYCCNT' "$RUN/renode.log" | tail -1)
tim=$(grep -A1 '^TIM2_CNT' "$RUN/renode.log" | tail -1)
loop=$([ "$r0" = 0x00000001 ] && echo PASS || echo FAIL)
order=$([ "$r1 $r2 $r3" = "0x00000100 0x00000200 0x00000300" ] && echo yes || echo no)
hz=$(( cyc / 2 ))
adv=$([ "$tim" != 0x00000000 ] && echo yes || echo no)
echo "EMBNET_P0_RENODE platform=nucleo_f767zi renode=1.17.0 can_loopback=$loop tx_order_by_id=$order rx_order=$r1,$r2,$r3 dwt_hz=$hz embassy_timer_advances=$adv run=$RUN" | tee "$RUN/summary.txt"
