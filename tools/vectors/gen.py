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


# ---------------------------------------------------------------- 04 datagram 與 COBS


def crc32c(data: bytes) -> int:
    return crc(data, 32, 0x1EDC6F41, 0xFFFFFFFF, True, True, 0xFFFFFFFF)


def crc64xz(data: bytes) -> int:
    return crc(data, 64, 0x42F0E1EBA9EA3693, (1 << 64) - 1, True, True, (1 << 64) - 1)


assert crc32c(b"123456789") == 0xE3069283
assert crc64xz(b"123456789") == 0x995DC9BBDF1939FA


def datagram(prio, kind_service, request, port, src, dst, tid, payload: bytes) -> bytes:
    flags = prio | (int(kind_service) << 3) | (int(request) << 4)
    head = bytes([0, flags]) + port.to_bytes(2, "little") + bytes([src, 0xFF if not kind_service else dst, tid & 0xFF, 0])
    body = head + payload
    return body + crc32c(body).to_bytes(4, "big")


def cobs_encode(data: bytes, trailing_01: bool = False) -> bytes:
    """COBS：每段最多 254 個非零位元組，前面放一個 code byte。

    資料以剛好 254 個非零位元組的區塊結束時，規格不再附加 0x01（trailing_01=False）；
    trailing_01=True 產生另一種常見寫法，給解碼端的相容性測試用。
    """
    out = bytearray()
    block = bytearray()
    ended_full = False
    for b in data:
        ended_full = False
        if b == 0:
            out.append(len(block) + 1)
            out += block
            block = bytearray()
        else:
            block.append(b)
            if len(block) == 254:
                out.append(255)
                out += block
                block = bytearray()
                ended_full = True
    if not ended_full or trailing_01:
        out.append(len(block) + 1)
        out += block
    return bytes(out)


def gen_datagram():
    cases = []
    for args in [(4, False, False, 100, 10, 0, 0, b""), (2, False, False, 7999, 127, 0, 255, pattern(20)),
                 (5, True, True, 510, 3, 42, 7, b"\x01"), (5, True, False, 510, 42, 3, 7, b"\x00\x00\x00")]:
        d = datagram(*args)
        prio, ks, req, port, src, dst, tid, p = args
        cases.append({"priority": prio, "kind": "service" if ks else "subject", "request": req, "port": port,
                      "source": src, "destination": dst if ks else 255, "transfer_id": tid, "payload": p.hex(),
                      "datagram": d.hex(), "uart": (cobs_encode(d) + b"\x00").hex()})
    cobs = []
    for data in [b"", b"\x00", b"\x00\x00", b"\x11\x22\x00\x33", b"\x11\x00\x00\x00", bytes(range(1, 255)),
                 bytes(range(1, 256)), bytes(range(0, 256)), bytes([0x01] * 300)]:
        cobs.append({"data": data.hex(), "encoded": cobs_encode(data).hex(),
                     "also_accept": cobs_encode(data, trailing_01=True).hex()})
    return {"spec": "04-datagram-links.md", "crc32c_check": "E3069283", "cases": cases, "cobs": cobs}


def type_hash(name: str, major: int, max_len: int, fields: str) -> int:
    norm = " ".join(fields.split())
    norm = ";".join(part.strip() for part in norm.split(";"))
    return crc64xz(f"{name}:{major}:{max_len}:{norm}".encode())


def gen_type_hash():
    cases = []
    for name, major, max_len, fields in [("demo.Speed", 1, 4, "int16 left; int16 right"),
                                         ("demo.Speed", 1, 4, "  int16   left ;int16 right  "),
                                         ("demo.Speed", 2, 4, "int16 left; int16 right"),
                                         ("demo.Empty", 1, 0, "")]:
        cases.append({"name": name, "major": major, "max_len": max_len, "fields": fields,
                      "type_hash": f"{type_hash(name, major, max_len, fields):016X}"})
    assert cases[0]["type_hash"] == cases[1]["type_hash"]
    return {"spec": "07-config.md §2", "crc64xz_check": "995DC9BBDF1939FA", "cases": cases}


# ---------------------------------------------------------------- 05 MAC：AES-128（FIPS 197）與 CMAC（RFC 4493）


def _xtime(a: int) -> int:
    return ((a << 1) ^ 0x1B) & 0xFF if a & 0x80 else a << 1


def _gmul(a: int, b: int) -> int:
    r = 0
    while b:
        if b & 1:
            r ^= a
        a = _xtime(a)
        b >>= 1
    return r


def _sbox():
    inv = [0] * 256
    for x in range(1, 256):
        for y in range(1, 256):
            if _gmul(x, y) == 1:
                inv[x] = y
                break
    s = []
    for x in range(256):
        b = inv[x]
        r = b
        for i in range(1, 5):
            r ^= ((b << i) | (b >> (8 - i))) & 0xFF
        s.append(r ^ 0x63)
    return s


SBOX = _sbox()
assert SBOX[0x00] == 0x63 and SBOX[0x53] == 0xED


def aes128_encrypt(key: bytes, block: bytes) -> bytes:
    rcon = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1B, 0x36]
    w = [list(key[i : i + 4]) for i in range(0, 16, 4)]
    for i in range(4, 44):
        t = list(w[i - 1])
        if i % 4 == 0:
            t = t[1:] + t[:1]
            t = [SBOX[b] for b in t]
            t[0] ^= rcon[i // 4 - 1]
        w.append([w[i - 4][j] ^ t[j] for j in range(4)])
    s = [list(block[c * 4 : c * 4 + 4]) for c in range(4)]  # s[column][row]

    def add(rnd):
        for c in range(4):
            for r in range(4):
                s[c][r] ^= w[rnd * 4 + c][r]

    add(0)
    for rnd in range(1, 11):
        for c in range(4):
            s[c] = [SBOX[b] for b in s[c]]
        for r in range(1, 4):
            row = [s[c][r] for c in range(4)]
            row = row[r:] + row[:r]
            for c in range(4):
                s[c][r] = row[c]
        if rnd != 10:
            for c in range(4):
                a = s[c]
                s[c] = [_gmul(a[0], 2) ^ _gmul(a[1], 3) ^ a[2] ^ a[3],
                        a[0] ^ _gmul(a[1], 2) ^ _gmul(a[2], 3) ^ a[3],
                        a[0] ^ a[1] ^ _gmul(a[2], 2) ^ _gmul(a[3], 3),
                        _gmul(a[0], 3) ^ a[1] ^ a[2] ^ _gmul(a[3], 2)]
        add(rnd)
    return bytes(b for c in s for b in c)


# FIPS 197 附錄 C.1
assert aes128_encrypt(bytes(range(16)), bytes.fromhex("00112233445566778899aabbccddeeff")).hex() == \
    "69c4e0d86a7b0430d8cdb78070b4c55a"


def cmac(key: bytes, msg: bytes) -> bytes:
    def dbl(b: bytes) -> bytes:
        v = int.from_bytes(b, "big") << 1
        if v >> 128:
            v = (v ^ 0x87) & ((1 << 128) - 1)
        return v.to_bytes(16, "big")

    l = aes128_encrypt(key, bytes(16))
    k1 = dbl(l)
    k2 = dbl(k1)
    n = max(1, (len(msg) + 15) // 16)
    complete = len(msg) > 0 and len(msg) % 16 == 0
    last = msg[(n - 1) * 16 :]
    if complete:
        last = bytes(a ^ b for a, b in zip(last, k1))
    else:
        last = last + b"\x80" + bytes(15 - len(last))
        last = bytes(a ^ b for a, b in zip(last, k2))
    x = bytes(16)
    for i in range(n - 1):
        x = aes128_encrypt(key, bytes(a ^ b for a, b in zip(x, msg[i * 16 : i * 16 + 16])))
    return aes128_encrypt(key, bytes(a ^ b for a, b in zip(x, last)))


_K = bytes.fromhex("2b7e151628aed2a6abf7158809cf4f3c")
assert cmac(_K, b"").hex() == "bb1d6929e95937287fa37d129b756746"  # RFC 4493 第 4 節 Example 1
assert cmac(_K, bytes.fromhex("6bc1bee22e409f96e93d7e117393172a")).hex() == "070a16b46b4d4144f79bdd9dd04a287c"  # Example 2


def mac_input(subject, src, th, seq, data):
    return subject.to_bytes(2, "little") + bytes([src]) + th.to_bytes(8, "little") + (seq & 0xFFFFFFFF).to_bytes(4, "little") + data


def mac_encode(key, tag_len, subject, src, th, seq, data):
    return bytes([seq & 0xFF]) + data + cmac(key, mac_input(subject, src, th, seq, data))[:tag_len]


def gen_mac():
    key = bytes.fromhex("000102030405060708090a0b0c0d0e0f")  # test-only
    S, SRC, TH = 1234, 42, 0x0123456789ABCDEF
    enc = []
    for tag_len in (4, 8):
        for seq, data in [(0, b""), (1, b"\x10\x20"), (256, b"\x01"), (70000, pattern(20))]:
            enc.append({"protection": f"mac{tag_len * 8}", "key": key.hex(), "subject_id": S, "source": SRC,
                        "type_hash": f"{TH:016X}", "seq": seq, "data": data.hex(),
                        "wire": mac_encode(key, tag_len, S, SRC, TH, seq, data).hex()})
    rfc = [{"key": _K.hex(), "msg": "", "cmac": cmac(_K, b"").hex()},
           {"key": _K.hex(), "msg": "6bc1bee22e409f96e93d7e117393172a",
            "cmac": cmac(_K, bytes.fromhex("6bc1bee22e409f96e93d7e117393172a")).hex()}]
    return {"spec": "05-mac.md §1", "note": "key 是 test-only", "rfc4493": rfc, "encode": enc}


# ---------------------------------------------------------------- 06 Transfer-ID 對應


def gen_tid_remap():
    """入口 Transfer-ID 序列 → 展開值 U → 出口 Transfer-ID。"""
    cases = []
    for name, m_in, m_out, seq in [("CAN 到 datagram，連續且繞圈", 32, 256, list(range(28, 32)) + list(range(0, 6))),
                                    ("CAN 到 datagram，有漏收", 32, 256, [0, 1, 5, 9, 30, 2]),
                                    ("datagram 到 CAN", 256, 32, [250, 251, 255, 0, 3, 40]),
                                    ("CAN 到 CAN", 32, 32, [7, 8, 9])]:
        u = None
        steps = []
        for t in seq:
            if u is None:
                u = t
            else:
                d = (t - (u % m_in)) % m_in
                u = (u + d) & 0xFFFFFFFF
            steps.append({"in": t, "u": u, "out": u % m_out})
            assert u % m_in == t
        cases.append({"name": name, "modulo_in": m_in, "modulo_out": m_out, "steps": steps})
    return {"spec": "06-forwarding.md §3", "cases": cases}


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "docs/spec/vectors")
    out.mkdir(parents=True, exist_ok=True)
    files = {"can-id.json": gen_can_id(), "segment.json": gen_segment(), "reassembly.json": gen_reassembly(),
             "e2e.json": gen_e2e(), "e2e-sync.json": gen_sync(), "heartbeat.json": gen_heartbeat(),
             "datagram.json": gen_datagram(), "type-hash.json": gen_type_hash(), "mac.json": gen_mac(),
             "tid-remap.json": gen_tid_remap()}
    for name, obj in files.items():
        obj["generator"] = "tools/vectors/gen.py"
        (out / name).write_text(json.dumps(obj, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    n = sum(len(o.get(k, [])) for o in files.values() for k in ("cases", "encode", "receive", "cobs", "rfc4493"))
    print(f"EMBNET_VECTORS files={len(files)} cases={n} crc16_check=29B1 crc32a_check=1697D06A crc32c_check=E3069283 crc64xz_check=995DC9BBDF1939FA aes_fips197=ok cmac_rfc4493=ok out={out}")


if __name__ == "__main__":
    main()
