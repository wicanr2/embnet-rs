#!/usr/bin/env bash
# P0 的相依授權清單（docs/plans/004 第七節）。用 cargo metadata 列出各探針的全部相依與 license 欄位。
#   tools/p0-license.sh
set -euo pipefail
cd "$(dirname "$0")/.."
RUN="runs/$(date +%Y%m%d-%H%M%S)-p0-license"
mkdir -p "$RUN"
for p in probes/p0-size probes/p0-renode; do
  tools/cargo.sh "$p" metadata --format-version 1 --offline > "$RUN/$(basename $p).json"
done
docker run --rm --network none --user "$(id -u):$(id -g)" --log-opt max-size=10m --log-opt max-file=3 \
  -v "$PWD/$RUN":/r embnet-py:3.13-r1 python -I -c '
import json, glob, re
pk = {}
for f in glob.glob("/r/*.json"):
    for p in json.load(open(f))["packages"]:
        if p["source"] is None:
            continue
        pk[(p["name"], p["version"])] = p.get("license") or "UNKNOWN"
PERM = {"MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "0BSD", "Unlicense", "Unicode-3.0", "Unicode-DFS-2016", "CC0-1.0", "BSL-1.0"}
def permissive(l):
    ids = [t for t in re.split(r"[\s()/]+|\bOR\b|\bAND\b|\bWITH\b", l) if t and t not in ("OR", "AND", "WITH")]
    return bool(ids) and all(t in PERM or t == "LLVM-exception" for t in ids)
rows = sorted(pk.items())
with open("/r/licenses.tsv", "w") as out:
    for (n, v), l in rows:
        out.write(f"{n}\t{v}\t{l}\n")
odd = [(n, v, l) for (n, v), l in rows if l == "UNKNOWN" or not permissive(l)]
for n, v, l in odd:
    print(f"REVIEW {n} {v} {l}")
unknown = sum(1 for _, _, l in odd if l == "UNKNOWN")
print(f"EMBNET_P0_LICENSE crates={len(rows)} non_permissive={len(odd) - unknown} unknown={unknown}")
' | sed "s|\$| run=$RUN|;/^REVIEW/s| run=.*||" | tee "$RUN/summary.txt"
