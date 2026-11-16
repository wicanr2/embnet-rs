"""解析 tools/p3-board.sh 讀回的 RAM，輸出通過行。用法：python -I p3_board_parse.py <run 目錄>"""
import math
import re
import sys

run = sys.argv[1]
txt = open(f"{run}/read.log").read()


def words(tag):
    sec = txt.split(tag + "\n", 1)[1]
    out = []
    for line in sec.splitlines():
        m = re.match(r"^[0-9a-f]{8}:\s+(.*)$", line.strip())
        if not m:
            if out:
                break
            continue
        out += [int(w, 16) for w in m.group(1).split()]
    return out


def hist(tag):
    h = []
    for w in words(tag):
        h += [w & 0xFFFF, w >> 16]
    return h


def q(h, p):
    n = sum(h)
    k = max(1, math.ceil(p * n))
    c = 0
    for i, v in enumerate(h):
        c += v
        if c >= k:
            return i
    return len(h) - 1


def ci(h):
    # p99 的 95% 信賴區間：排序統計量的常態近似（docs/40-rust-embedded/04 第二節）
    n = sum(h)
    k = 0.99 * n
    w = 1.96 * math.sqrt(n * 0.99 * 0.01)
    return q(h, max(k - w, 1) / n), q(h, min(k + w, n) / n)


r = words("RESULT")
label = run.replace("/r", sys.argv[2]) if len(sys.argv) > 2 else run
if r[0] != 0x0B0B0B0B:
    print(f"EMBNET_HOP FAIL marker={r[0]:#x} progress={r[1]:#x} run={label}")
    sys.exit(1)
for name, h, off, inn, out in [("single_bus", hist("HIST_A"), 2, "can1", "can1"), ("bridge_hop", hist("HIST_B"), 6, "can1", "can3")]:
    sent, got, bad, maxc = r[off:off + 4]
    lo, hi = ci(h)
    ok = got == sent and bad == 0 and sum(h) == got and h[-1] == 0
    status = "PASS" if ok else "FAIL"
    print(f"EMBNET_HOP {status} in={inn} out={out} case={name} env=board-loopback bitrate=1000000 frames_per_transfer=1 payload_bytes=7 "
          f"samples={sum(h)} sent={sent} p50_us={q(h, 0.5)} p99_us={q(h, 0.99)} p99_ci_us={lo}-{hi} max_us={maxc / 216:.1f} "
          f"e2e_bad={bad} run={label}")
print(f"EMBNET_BRIDGE_STATS env=board-loopback forwarded={r[10]} no_route={r[11]} local={r[12]} queue_dropped={r[13]} "
      f"rate_limited={r[14]} run={label}")
