//! 用 `docs/spec/vectors/` 的 test vector 驗證實作。vector 由獨立的 Python 程式產生（`tools/vectors/gen.py`）。
//! 測試可以 panic，所以放寬核心的 lint。
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects,
    missing_docs
)]

use embnet_core::can::{CanId, Mtu, NodeId, Priority, Reassembler, Segmenter, ServiceId, SubjectId};
use embnet_core::e2e::{self, DataId, SeqReceiver, Status};
use embnet_core::heartbeat::Heartbeat;
use serde_json::Value;

fn load(name: &str) -> Value {
    let path = format!("{}/../../docs/spec/vectors/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).expect(&path)).unwrap()
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

fn u(v: &Value) -> u64 {
    v.as_u64().unwrap()
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap()
}

#[test]
fn can_id() {
    let v = load("can-id.json");
    let cases = v["cases"].as_array().unwrap();
    for c in cases {
        let expect = u32::from_str_radix(s(&c["can_id"]), 16).unwrap();
        let p = Priority::new(u(&c["priority"]) as u8).unwrap();
        let src = NodeId::new(u(&c["source"]) as u8).unwrap();
        let id = if s(&c["kind"]) == "subject" {
            CanId::Subject { priority: p, subject: SubjectId::new(u(&c["subject_id"]) as u16).unwrap(), source: src }
        } else {
            CanId::Service {
                priority: p,
                request: c["request"].as_bool().unwrap(),
                service: ServiceId::new(u(&c["service_id"]) as u16).unwrap(),
                destination: NodeId::new(u(&c["destination"]) as u8).unwrap(),
                source: src,
            }
        };
        assert_eq!(id.encode(), expect, "{c}");
        assert_eq!(CanId::decode(expect), Ok(id), "{c}");
    }
    println!("EMBNET_VEC can-id cases={}", cases.len());
}

#[test]
fn can_id_reserved_bits_rejected() {
    // Subject 的位元 24-20、Service 的位元 23 是保留位元
    assert!(CanId::decode(1 << 20).is_err());
    assert!(CanId::decode((1 << 25) | (1 << 23)).is_err());
    assert!(CanId::decode(1 << 29).is_err());
}

#[test]
fn segment() {
    let v = load("segment.json");
    let cases = v["cases"].as_array().unwrap();
    for c in cases {
        let mtu = if u(&c["mtu"]) == 8 { Mtu::Classic } else { Mtu::Fd };
        let payload = hex(s(&c["payload"]));
        let expect: Vec<Vec<u8>> = c["frames"].as_array().unwrap().iter().map(|f| hex(s(f))).collect();
        let mut seg = Segmenter::new(&payload, mtu, u(&c["transfer_id"]) as u8);
        assert_eq!(seg.frame_count(), expect.len(), "{c}");
        let mut got = Vec::new();
        let mut buf = [0u8; 64];
        while let Some(n) = seg.next_frame(&mut buf) {
            got.push(buf[..n].to_vec());
        }
        assert_eq!(got, expect, "mtu={} len={}", u(&c["mtu"]), payload.len());

        // 反向：把框餵給重組器，要拿回原 payload（CAN FD 的補位在尾端）
        let mut r = Reassembler::<512>::new();
        let mut out = None;
        for f in &expect {
            if let Some(a) = r.push(0, 1_000_000, f).unwrap() {
                out = Some(a.payload.to_vec());
            }
        }
        let out = out.expect("沒有完成重組");
        assert_eq!(&out[..payload.len()], &payload[..]);
        assert!(out[payload.len()..].iter().all(|&b| b == 0));
    }
    println!("EMBNET_VEC segment cases={}", cases.len());
}

#[test]
fn reassembly() {
    let v = load("reassembly.json");
    let cases = v["cases"].as_array().unwrap();
    for c in cases {
        let mut r = Reassembler::<64>::new();
        let mut delivered = Vec::new();
        for (i, f) in c["frames"].as_array().unwrap().iter().enumerate() {
            // 每框間隔 1 ms，遠小於 1 秒的 Transfer-ID 逾時
            if let Ok(Some(a)) = r.push(i as u64 * 1000, 1_000_000, &hex(s(f))) {
                delivered.push(hex::encode(a.payload));
            }
        }
        let expect: Vec<String> = c["delivered"].as_array().unwrap().iter().map(|x| s(x).to_string()).collect();
        assert_eq!(delivered, expect, "{}", s(&c["name"]));
    }
    println!("EMBNET_VEC reassembly cases={}", cases.len());
}

mod hex {
    pub fn encode(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }
}

fn data_id(c: &Value) -> DataId {
    DataId {
        subject_id: u(&c["subject_id"]) as u16,
        source: u(&c["source"]) as u8,
        type_hash: u64::from_str_radix(s(&c["type_hash"]), 16).unwrap(),
    }
}

#[test]
fn e2e_encode_and_receive() {
    let v = load("e2e.json");
    let enc = v["encode"].as_array().unwrap();
    for c in enc {
        let data = hex(s(&c["data"]));
        let mut out = [0u8; 64];
        let n = e2e::encode(&data_id(c), u(&c["seq"]) as u32, &data, &mut out).unwrap();
        assert_eq!(hex::encode(&out[..n]), s(&c["wire"]), "{c}");
    }
    let rx = v["receive"].as_array().unwrap();
    for c in rx {
        let mut r = SeqReceiver::new(u(&c["max_delta"]) as u8);
        if c["synced"].as_bool().unwrap() {
            r.sync((u(&c["last"]) as u32).wrapping_add(1));
        }
        let wire = hex(s(&c["wire"]));
        let (st, _) = r.check(&data_id(c), &wire);
        assert_eq!(st as u8, u(&c["status"]) as u8, "{}", s(&c["name"]));
        if r.synced() {
            assert_eq!(u64::from(r.last()), u(&c["new_last"]), "{}", s(&c["name"]));
        }
    }
    println!("EMBNET_VEC e2e encode={} receive={}", enc.len(), rx.len());
}

#[test]
fn e2e_sync() {
    let v = load("e2e-sync.json");
    let cases = v["cases"].as_array().unwrap();
    for c in cases {
        let src = u(&c["source"]) as u8;
        let entries: Vec<(u16, u32)> = c["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (u(&e["subject_id"]) as u16, u(&e["next_seq"]) as u32))
            .collect();
        let mut out = [0u8; 256];
        let n = e2e::sync_encode(src, &entries, &mut out).unwrap();
        assert_eq!(hex::encode(&out[..n]), s(&c["payload"]));
        let back: Vec<(u16, u32)> = e2e::sync_decode(src, &out[..n]).unwrap().collect();
        assert_eq!(back, entries);
        // 來源不符時 CRC 不過
        assert!(e2e::sync_decode(src ^ 1, &out[..n]).is_none());
    }
    println!("EMBNET_VEC e2e-sync cases={}", cases.len());
}

#[test]
fn heartbeat() {
    let v = load("heartbeat.json");
    let cases = v["cases"].as_array().unwrap();
    for c in cases {
        let hb = Heartbeat {
            uptime: u(&c["uptime"]) as u32,
            health: u(&c["health"]) as u8,
            mode: u(&c["mode"]) as u8,
            fault_log_unread: c["fault_log_unread"].as_bool().unwrap(),
            auth_disabled: c["auth_disabled"].as_bool().unwrap(),
            can_state: u(&c["can_state"]) as u8,
            vendor: u(&c["vendor"]) as u8,
        };
        let p = hb.encode();
        assert_eq!(hex::encode(&p), s(&c["payload"]));
        assert_eq!(Heartbeat::decode(&p), Some(hb));
        let mut buf = [0u8; 8];
        let n = Segmenter::new(&p, Mtu::Classic, 0).next_frame(&mut buf).unwrap();
        assert_eq!(hex::encode(&buf[..n]), s(&c["frame_classic"]));
    }
    println!("EMBNET_VEC heartbeat cases={}", cases.len());
}

#[test]
fn e2e_state_machine_basic() {
    use e2e::{StateMachine, StateParams, StreamState};
    let mut m = StateMachine::new(StateParams::default());
    assert_eq!(m.push(Status::NoSync), StreamState::NoData);
    assert_eq!(m.push(Status::Ok), StreamState::Init);
    assert_eq!(m.push(Status::Ok), StreamState::Valid);
    m.push(Status::Error);
    m.push(Status::Error);
    assert_eq!(m.state(), StreamState::Valid);
    assert_eq!(m.push(Status::Error), StreamState::Invalid);
    for _ in 0..4 {
        m.push(Status::Ok);
    }
    assert_eq!(m.state(), StreamState::Invalid, "窗內還有錯，不能回到 VALID");
    for _ in 0..4 {
        m.push(Status::Ok);
    }
    assert_eq!(m.state(), StreamState::Valid);
}

#[test]
fn datagram_and_cobs() {
    use embnet_core::datagram::{self, CobsReceiver, Header};
    let v = load("datagram.json");
    let cases = v["cases"].as_array().unwrap();
    for c in cases {
        let h = Header {
            priority: u(&c["priority"]) as u8,
            service: s(&c["kind"]) == "service",
            request: c["request"].as_bool().unwrap(),
            port: u(&c["port"]) as u16,
            source: u(&c["source"]) as u8,
            destination: u(&c["destination"]) as u8,
            transfer_id: u(&c["transfer_id"]) as u8,
        };
        let payload = hex(s(&c["payload"]));
        let mut out = [0u8; 512];
        let n = datagram::encode(&h, &payload, &mut out).unwrap();
        assert_eq!(hex::encode(&out[..n]), s(&c["datagram"]));
        let (h2, p2) = datagram::decode(&out[..n]).unwrap();
        assert_eq!((h2, p2), (h, &payload[..]));

        let mut framed = [0u8; 600];
        let m = datagram::cobs_frame(&out[..n], &mut framed).unwrap();
        assert_eq!(hex::encode(&framed[..m]), s(&c["uart"]));
        let mut rx = CobsReceiver::<600>::new();
        let mut got = None;
        for &b in std::iter::once(&0u8).chain(framed[..m].iter()) {
            if let Some(r) = rx.push(b) {
                got = Some(r.unwrap().to_vec());
            }
        }
        assert_eq!(got.unwrap(), out[..n].to_vec());

        // 任一位元組毀損都要被 CRC 抓到
        for i in 0..n {
            let mut bad = out[..n].to_vec();
            bad[i] ^= 0x01;
            assert!(datagram::decode(&bad).is_err(), "byte {i}");
        }
    }
    let cobs = v["cobs"].as_array().unwrap();
    for c in cobs {
        let data = hex(s(&c["data"]));
        let mut out = [0u8; 600];
        let n = datagram::cobs_frame(&data, &mut out).unwrap();
        assert_eq!(hex::encode(&out[..n - 1]), s(&c["encoded"]));
        // 兩種寫法都要能解回原資料
        for key in ["encoded", "also_accept"] {
            let mut rx = CobsReceiver::<600>::new();
            let mut got = None;
            for b in hex(s(&c[key])).into_iter().chain(std::iter::once(0)) {
                if let Some(r) = rx.push(b) {
                    got = Some(r.unwrap().to_vec());
                }
            }
            assert_eq!(got.unwrap_or_default(), data, "{key}");
        }
    }
    println!("EMBNET_VEC datagram cases={} cobs={}", cases.len(), cobs.len());
}

#[test]
fn type_hash() {
    let v = load("type-hash.json");
    let cases = v["cases"].as_array().unwrap();
    for c in cases {
        // 正規化依 docs/spec/07-config.md 第二節；正式實作在 embnet-config
        let fields: String = s(&c["fields"]).split_whitespace().collect::<Vec<_>>().join(" ");
        let fields: Vec<&str> = fields.split(';').map(str::trim).collect();
        let canon = format!("{}:{}:{}:{}", s(&c["name"]), u(&c["major"]), u(&c["max_len"]), fields.join(";"));
        let h = embnet_core::crc::crc64_xz(canon.as_bytes());
        assert_eq!(format!("{h:016X}"), s(&c["type_hash"]), "{canon}");
    }
    println!("EMBNET_VEC type-hash cases={}", cases.len());
}

#[test]
fn mac() {
    use embnet_core::mac::{self, TagLen};
    let v = load("mac.json");
    let enc = v["encode"].as_array().unwrap();
    for c in enc {
        let key: [u8; 16] = hex(s(&c["key"])).try_into().unwrap();
        let tag = if s(&c["protection"]) == "mac32" { TagLen::Bits32 } else { TagLen::Bits64 };
        let data = hex(s(&c["data"]));
        let seq = u(&c["seq"]) as u32;
        let id = data_id(c);
        let mut out = [0u8; 64];
        let n = mac::encode(&key, tag, &id, seq, &data, &mut out).unwrap();
        assert_eq!(hex::encode(&out[..n]), s(&c["wire"]), "{c}");

        // 接收：同步在前一筆，應為 OK；改一個位元或換金鑰應為 ERROR
        let mut rx = SeqReceiver::new(3);
        rx.sync(seq);
        let (st, d) = mac::check(&mut rx, &key, tag, &id, &out[..n]);
        assert_eq!((st, d), (Status::Ok, Some(&data[..])));
        let mut rx = SeqReceiver::new(3);
        rx.sync(seq);
        let mut bad = out[..n].to_vec();
        *bad.last_mut().unwrap() ^= 1;
        assert_eq!(mac::check(&mut rx, &key, tag, &id, &bad).0, Status::Error);
        let mut rx = SeqReceiver::new(3);
        rx.sync(seq);
        let mut other = key;
        other[0] ^= 1;
        assert_eq!(mac::check(&mut rx, &other, tag, &id, &out[..n]).0, Status::Error);
    }
    println!("EMBNET_VEC mac encode={}", enc.len());
}

#[test]
fn cmac_rfc4493() {
    // RFC 4493 第 4 節 Example 1 與 2：用 Data ID 以外的路徑直接算，確認底層 AES-CMAC 正確
    use aes::Aes128;
    use cmac::{Cmac, KeyInit, Mac};
    let v = load("mac.json");
    for c in v["rfc4493"].as_array().unwrap() {
        let mut m = <Cmac<Aes128> as KeyInit>::new_from_slice(&hex(s(&c["key"]))).unwrap();
        m.update(&hex(s(&c["msg"])));
        assert_eq!(hex::encode(&m.finalize().into_bytes()), s(&c["cmac"]));
    }
}

#[test]
fn tid_remap() {
    use embnet_core::remap::TidRemap;
    let v = load("tid-remap.json");
    let cases = v["cases"].as_array().unwrap();
    for c in cases {
        let (mi, mo) = (u(&c["modulo_in"]) as u32, u(&c["modulo_out"]) as u32);
        let mut r = TidRemap::new();
        for st in c["steps"].as_array().unwrap() {
            assert_eq!(u64::from(r.ingress(u(&st["in"]) as u8, mi)), u(&st["u"]), "{}", s(&c["name"]));
            assert_eq!(u64::from(r.egress(mo).unwrap()), u(&st["out"]), "{}", s(&c["name"]));
        }
    }
    println!("EMBNET_VEC tid-remap cases={}", cases.len());
}
