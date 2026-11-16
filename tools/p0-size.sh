#!/usr/bin/env bash
# P0 體積量測（docs/plans/004 第四節）。
#   tools/p0-size.sh
# 量法是相減：同一個裸機外殼，不碰協定的 baseline 與各實作的探針相減，才是協定本身的成本。
# 配置器的 heap 或記憶池在 .bss 裡，另外扣掉並以 heap= 列出。
set -euo pipefail
cd "$(dirname "$0")/.."
RUN="runs/$(date +%Y%m%d-%H%M%S)-p0-size"
mkdir -p "$RUN"
IMAGE="${IMAGE:-embnet-rust:1.100.0-r1}"

for b in size-baseline size-canadensis size-emcyphal size-embnet size-embnet-full; do
  tools/cargo.sh probes/p0-size build --release -p "$b" > "$RUN/$b.build.log" 2>&1
  tools/cargo.sh probes/p0-size size --release -p "$b" -- -A > "$RUN/$b.size" 2>"$RUN/$b.size.log"
done

docker run --rm --network none --user "$(id -u):$(id -g)" --cpus 4 --memory 2g --pids-limit 512 \
  --log-opt max-size=10m --log-opt max-file=3 \
  -v "$PWD/probes/p0-size-c":/w -w /w "$IMAGE" ./build.sh > "$RUN/c.build.log" 2>&1
cp probes/p0-size-c/build/size-baseline-c.size probes/p0-size-c/build/size-libcanard.size "$RUN/"
git -C probes/p0-size-c/third_party/libcanard log -1 --format=%H > "$RUN/libcanard.commit"

sec() { awk -v s="$2" '$1==s {print $2; f=1} END {if (!f) print 0}' "$1"; }
line() { # impl 探針檔 baseline檔 heap
  local impl=$1 f=$2 base=$3 heap=$4
  local t r d b
  t=$(( $(sec "$f" .text) - $(sec "$base" .text) ))
  r=$(( $(sec "$f" .rodata) - $(sec "$base" .rodata) ))
  d=$(( $(sec "$f" .data) - $(sec "$base" .data) ))
  b=$(( $(sec "$f" .bss) - $(sec "$base" .bss) - heap ))
  echo "EMBNET_P0_SIZE impl=$impl target=thumbv7em-none-eabihf cpu=cortex-m7 text=$t rodata=$r data=$d bss=$b heap=$heap profile=z,lto run=$RUN"
}
{
  line canadensis "$RUN/size-canadensis.size" "$RUN/size-baseline.size" 16384
  line emcyphal   "$RUN/size-emcyphal.size"   "$RUN/size-baseline.size" 0
  line libcanard  "$RUN/size-libcanard.size"  "$RUN/size-baseline-c.size" 4096
  line embnet-node "$RUN/size-embnet.size" "$RUN/size-baseline.size" 0
  line embnet-node-full "$RUN/size-embnet-full.size" "$RUN/size-baseline.size" 0
} | tee "$RUN/summary.txt"
