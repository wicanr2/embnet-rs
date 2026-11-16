#!/usr/bin/env python3
"""embnet 規格 0.1 的 test vector 產生器。

依 docs/spec/ 獨立撰寫，不共用 Rust 的程式碼（docs/plans/003 K-10）。
只用 Python 標準函式庫。輸出到 docs/spec/vectors/*.json。

    python -I tools/vectors/gen.py <輸出目錄>
"""

import json
import sys
from pathlib import Path

# ---------------------------------------------------------------- CRC（逐位元，照 RevEng 參數）


def _reflect(v: int, width: int) -> int:
    r = 0
    for i in range(width):
        if v & (1 << i):
            r |= 1 << (width - 1 - i)
    return r


def crc(data: bytes, width: int, poly: int, init: int, refin: bool, refout: bool, xorout: int) -> int:
    top = 1 << (width - 1)
    mask = (1 << width) - 1
    reg = init
    for byte in data:
        if refin:
            byte = _reflect(byte, 8)
        reg ^= byte << (width - 8)
        for _ in range(8):
            reg = ((reg << 1) ^ poly) & mask if reg & top else (reg << 1) & mask
    if refout:
        reg = _reflect(reg, width)
    return reg ^ xorout


def crc16(data: bytes) -> int:  # CRC-16/IBM-3740
    return crc(data, 16, 0x1021, 0xFFFF, False, False, 0x0000)


def crc32a(data: bytes) -> int:  # CRC-32/AUTOSAR
    return crc(data, 32, 0xF4ACFB13, 0xFFFFFFFF, True, True, 0xFFFFFFFF)


assert crc16(b"123456789") == 0x29B1
assert crc32a(b"123456789") == 0x1697D06A

# ---------------------------------------------------------------- 01 CAN 傳輸


def can_id_subject(prio: int, subject: int, src: int) -> int:
    return (prio << 26) | (0 << 25) | (subject << 7) | src


def can_id_service(prio: int, request: bool, service: int, dst: int, src: int) -> int:
    return (prio << 26) | (1 << 25) | (int(request) << 24) | (service << 14) | (dst << 7) | src


FD_LENGTHS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64]


def fd_round_up(n: int) -> int:
    for v in FD_LENGTHS:
        if v >= n:
            return v
    raise ValueError(n)


def tail(sot: bool, eot: bool, toggle: bool, tid: int) -> int:
    return (int(sot) << 7) | (int(eot) << 6) | (int(toggle) << 5) | (tid & 0x1F)


def segment(payload: bytes, mtu: int, tid: int) -> list[bytes]:
    m = mtu - 1
    if len(payload) <= m:
        body = payload
        if mtu > 8:
            body = body + bytes(fd_round_up(len(body) + 1) - 1 - len(body))
        return [body + bytes([tail(True, True, True, tid)])]
    stream = payload
    if mtu > 8:
        # 最後一框（含 CRC 與 Tail byte）長度要是合法的 CAN FD 長度；補位放在 CRC 之前
        while True:
            last = (len(stream) + 2) % m or m
            if fd_round_up(last + 1) == last + 1:
                break
            stream = stream + b"\x00"
    stream = stream + crc16(stream).to_bytes(2, "big")
    frames = []
    toggle = True
    for i in range(0, len(stream), m):
        chunk = stream[i : i + m]
        frames.append(chunk + bytes([tail(i == 0, i + m >= len(stream), toggle, tid)]))
        toggle = not toggle
    return frames


def gen_can_id():
    cases = []
    for prio, subject, src in [(0, 0, 0), (4, 8190, 42), (7, 8191, 127), (3, 1234, 5), (4, 7509, 1)]:
        cases.append({"kind": "subject", "priority": prio, "subject_id": subject, "source": src,
                      "can_id": f"{can_id_subject(prio, subject, src):08X}"})
    for prio, req, svc, dst, src in [(4, True, 510, 3, 42), (4, False, 510, 42, 3), (0, True, 0, 0, 0), (7, False, 511, 127, 127)]:
        cases.append({"kind": "service", "priority": prio, "request": req, "service_id": svc, "destination": dst,
                      "source": src, "can_id": f"{can_id_service(prio, req, svc, dst, src):08X}"})
    return {"spec": "01-can-transport.md §1", "cases": cases}


def pattern(n: int) -> bytes:
    return bytes((i * 7 + 3) & 0xFF for i in range(n))


def gen_segment():
    cases = []
    for mtu, n, tid in [(8, 0, 0), (8, 1, 1), (8, 7, 2), (8, 8, 3), (8, 12, 4), (8, 13, 5), (8, 14, 6), (8, 20, 31),
                        (64, 10, 7), (64, 63, 8), (64, 64, 9), (64, 70, 10), (64, 150, 11)]:
        p = pattern(n)
        cases.append({"mtu": mtu, "transfer_id": tid, "payload": p.hex(),
                      "frames": [f.hex() for f in segment(p, mtu, tid)]})
    return {"spec": "01-can-transport.md §2-3", "payload_pattern": "byte[i] = (7*i + 3) mod 256", "cases": cases}


def gen_reassembly():
    """每個案例：依序收到的框（同一個 session），以及預期交給應用的 payload 清單。"""
    p13 = pattern(13)
    good = segment(p13, 8, 5)  # 3 框
    cases = []
    cases.append({"name": "多框正常", "frames": [f.hex() for f in good], "delivered": [p13.hex()]})
    cases.append({"name": "單框正常", "frames": [segment(b"\x01\x02", 8, 0)[0].hex()], "delivered": ["0102"]})
    bad_crc = bytearray(good[2])
    bad_crc[0] ^= 0x01
    cases.append({"name": "CRC 錯", "frames": [good[0].hex(), good[1].hex(), bytes(bad_crc).hex()], "delivered": []})
    cases.append({"name": "沒有 SOT 的中間框", "frames": [good[1].hex(), good[2].hex()], "delivered": []})
    cases.append({"name": "重複的中間框被丟棄，之後仍完成", "frames": [good[0].hex(), good[1].hex(), good[1].hex(), good[2].hex()],
                  "delivered": [p13.hex()]})
    other = segment(pattern(9), 8, 6)
    cases.append({"name": "中途出現新 Transfer-ID 的 SOT，取代舊的", "frames": [good[0].hex(), other[0].hex(), other[1].hex()],
                  "delivered": [pattern(9).hex()]})
    sot_bad_toggle = bytearray(good[0])
    sot_bad_toggle[-1] &= ~0x20
    cases.append({"name": "SOT 框的 toggle 為 0", "frames": [bytes(sot_bad_toggle).hex(), good[1].hex(), good[2].hex()],
                  "delivered": []})
    single = segment(b"\xAA", 8, 3)[0]
    cases.append({"name": "重複的單框（逾時內）", "frames": [single.hex(), single.hex()], "delivered": ["aa"]})
    cases.append({"name": "空框", "frames": [""], "delivered": []})
    return {"spec": "01-can-transport.md §5-6", "note": "所有框屬於同一個 session，時間差都遠小於 Transfer-ID 逾時；extent 足夠",
            "cases": cases}


# ---------------------------------------------------------------- 02 E2E


def e2e_crc(subject: int, src: int, type_hash: int, seq: int, data: bytes) -> int:
    pre = subject.to_bytes(2, "little") + bytes([src]) + type_hash.to_bytes(8, "little") + (seq & 0xFFFFFFFF).to_bytes(4, "little")
    return crc32a(pre + data)


def e2e_encode(subject: int, src: int, type_hash: int, seq: int, data: bytes) -> bytes:
    return bytes([seq & 0xFF]) + data + e2e_crc(subject, src, type_hash, seq, data).to_bytes(4, "big")


def e2e_check(subject, src, type_hash, synced, last, wire: bytes, max_delta=3):
    if len(wire) < 5:
        return 4, last
    if not synced:
        return 5, last
    low, data, c = wire[0], wire[1:-4], int.from_bytes(wire[-4:], "big")
    d = (low - (last & 0xFF)) % 256
    if d == 0:
        return (2 if e2e_crc(subject, src, type_hash, last, data) == c else 4), last
    cand = (last + d) & 0xFFFFFFFF
    if e2e_crc(subject, src, type_hash, cand, data) != c:
        return 4, last
    status = 0 if d == 1 else (1 if d <= max_delta else 3)
    return status, cand


def gen_e2e():
    S, SRC, TH = 1234, 42, 0x0123456789ABCDEF
    enc = []
    for seq, data in [(0, b""), (1, b"\x10\x20"), (255, b"\x01"), (256, b"\x01"), (0xFFFFFFFF, b"\xDE\xAD\xBE\xEF"), (70000, pattern(20))]:
        enc.append({"subject_id": S, "source": SRC, "type_hash": f"{TH:016X}", "seq": seq, "data": data.hex(),
                    "wire": e2e_encode(S, SRC, TH, seq, data).hex()})
    rx = []

    def case(name, synced, last, seq, data=b"\x55\x66", wire=None, subject=S, src=SRC, th=TH):
        w = wire if wire is not None else e2e_encode(S, SRC, TH, seq, data)
        st, new = e2e_check(subject, src, th, synced, last, w)
        rx.append({"name": name, "subject_id": subject, "source": src, "type_hash": f"{th:016X}", "synced": synced,
                   "last": last, "wire": w.hex(), "max_delta": 3, "status": st, "new_last": new})

    case("下一筆", True, 99, 100)
    case("漏 1 筆，d = 2", True, 99, 101)
    case("漏 2 筆，d = 3 等於 max_delta", True, 99, 102)
    case("漏 3 筆，d = 4 超過 max_delta", True, 99, 103)
    case("重複", True, 100, 100)
    case("跨 256 邊界", True, 255, 256)
    case("跨 2^32 邊界", True, 0xFFFFFFFF, 0)
    case("漏收 300 筆，高位重建錯誤", True, 100, 400)
    case("未同步", False, 0, 5)
    case("Data ID 不符：別的 Subject", True, 99, 100, subject=S + 1)
    case("Data ID 不符：別的來源", True, 99, 100, src=SRC + 1)
    case("Type hash 不符", True, 99, 100, th=TH ^ 1)
    w = bytearray(e2e_encode(S, SRC, TH, 100, b"\x55\x66"))
    w[1] ^= 0x80
    case("資料毀損", True, 99, 100, wire=bytes(w))
    case("太短", True, 99, 100, wire=b"\x64\x00\x00\x00")
    return {"spec": "02-e2e.md §1-4", "status_codes": {"OK": 0, "OKSOMELOST": 1, "REPEATED": 2, "WRONGSEQUENCE": 3,
                                                       "ERROR": 4, "NOSYNC": 5}, "encode": enc, "receive": rx}


def sync_encode(src: int, entries: list[tuple[int, int]]) -> bytes:
    body = bytes([len(entries)]) + b"".join(s.to_bytes(2, "little") + q.to_bytes(4, "little") for s, q in entries)
    pre = (8189).to_bytes(2, "little") + bytes([src]) + bytes(8)
    return body + crc32a(pre + body).to_bytes(4, "big")


def gen_sync():
    cases = []
    for src, entries in [(42, []), (42, [(1234, 0)]), (7, [(1, 1), (2, 0xFFFFFFFF), (7999, 70000)])]:
        cases.append({"source": src, "entries": [{"subject_id": s, "next_seq": q} for s, q in entries],
                      "payload": sync_encode(src, entries).hex()})
    return {"spec": "02-e2e.md §5", "subject_id": 8189, "cases": cases}


# ---------------------------------------------------------------- 03 Heartbeat


def heartbeat(uptime, health, mode, unread, auth_disabled, can_state, vendor) -> bytes:
    status = health | (mode << 2) | (int(unread) << 5) | (int(auth_disabled) << 6)
    return uptime.to_bytes(4, "little") + bytes([status, can_state, vendor])


def gen_heartbeat():
    cases = []
    for args in [(0, 0, 1, False, False, 0, 0), (3600, 0, 0, False, False, 0, 0), (0xFFFFFFFF, 3, 3, True, True, 3, 0xFF),
                 (12345, 1, 2, True, False, 1, 0x5A)]:
        up, h, m, u, a, c, v = args
        cases.append({"uptime": up, "health": h, "mode": m, "fault_log_unread": u, "auth_disabled": a, "can_state": c,
                      "vendor": v, "payload": heartbeat(*args).hex(),
                      "frame_classic": segment(heartbeat(*args), 8, 0)[0].hex(),
                      "can_id_from_node_42": f"{can_id_subject(4, 8190, 42):08X}"})
    return {"spec": "03-node-services.md §2", "subject_id": 8190, "cases": cases}


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "docs/spec/vectors")
    out.mkdir(parents=True, exist_ok=True)
    files = {"can-id.json": gen_can_id(), "segment.json": gen_segment(), "reassembly.json": gen_reassembly(),
             "e2e.json": gen_e2e(), "e2e-sync.json": gen_sync(), "heartbeat.json": gen_heartbeat()}
    for name, obj in files.items():
        obj["generator"] = "tools/vectors/gen.py"
        (out / name).write_text(json.dumps(obj, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    n = sum(len(o.get("cases", [])) + len(o.get("encode", [])) + len(o.get("receive", [])) for o in files.values())
    print(f"EMBNET_VECTORS files={len(files)} cases={n} crc16_check=29B1 crc32a_check=1697D06A out={out}")


if __name__ == "__main__":
    main()
