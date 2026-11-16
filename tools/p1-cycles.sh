#!/usr/bin/env bash
# P1 的每步成本（docs/plans/006）。在 Renode 上跑，輸出的是推算的指令數，不是實機週期數。
#   tools/p1-cycles.sh
set -euo pipefail
cd "$(dirname "$0")/.."
RUN="runs/$(date +%Y%m%d-%H%M%S)-p1-cycles"
mkdir -p "$RUN"
ELF=target/thumbv7em-none-eabihf/release/p1-cycles

tools/cargo.sh probes/p1-cycles build --release > "$RUN/build.log" 2>&1
ADDR=$(docker run --rm --network none --user "$(id -u):$(id -g)" --log-opt max-size=10m --log-opt max-file=3 \
  -v "$PWD/probes/p1-cycles":/w:ro embnet-rust:1.100.0-r1 arm-none-eabi-nm "/w/$ELF" | awk '$3=="P1_RESULT"{print $1}')
test "$ADDR" = "20000004" || { echo "P1_RESULT 位址改變：$ADDR，請更新 renode/p1.resc" >&2; exit 1; }

timeout 300 docker run --rm --network none --user "$(id -u):$(id -g)" -e HOME=/tmp --cpus 2 --memory 2g --pids-limit 512 \
  --log-opt max-size=10m --log-opt max-file=3 -v "$PWD/probes/p1-cycles":/w:ro embnet-renode:1.17.0-r1 \
  renode --disable-xwt --console --plain -e "\$elf=@/w/$ELF; include @/w/renode/p1.resc" 2>&1 | tr -d '\r' > "$RUN/renode.log"

mapfile -t V < <(sed -n '/P1_RESULT_BEGIN/,/P1_RESULT_END/p' "$RUN/renode.log" | grep '^0x')
[ "${V[0]}" = 0x0C1C0C1C ] || { echo "韌體沒有跑完：${V[0]}" >&2; exit 1; }
MIPS=$(( $(grep -A1 '^MIPS' "$RUN/renode.log" | tail -1) ))
NAMES=(mac32_gen mac32_verify e2e32_gen e2e32_verify canid_decode segment_9B reassemble_2f datagram_decode_20B mac32_gen_cached mac32_verify_cached mac_key_expand)
line="EMBNET_P1_COST env=renode-1.17.0 dwt_hz=72000000 mips=$MIPS unit=instructions k=$(( ${V[1]} ))"
for i in "${!NAMES[@]}"; do
  c=$(( ${V[$((i + 2))]} ))
  line="$line ${NAMES[$i]}=$(( c * MIPS / 72 ))"
done
echo "$line run=$RUN" | tee "$RUN/summary.txt"
