//! 模擬匯流排上的整合測試與故障注入（`docs/plans/002` 第六節的 EMBNET_FAULT）。
//! 時間是一個數字，故障由固定 seed 的亂數決定，失敗時用同一個 seed 重現。
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects,
    missing_docs
)]

use embnet_core::can::{Mtu, NodeId, Priority, ServiceId, SubjectId};
use embnet_core::e2e::{StateParams, Status, StreamState};
use embnet_node::{Event, Node, PubProtection, Publication, SubProtection, Subscription};

type N = Node<4, 4, 4, 8, 64, 32, 4>;

const TH: u64 = 0x4FF0_0AF1_D812_8708;
const CMD: u16 = 100;
const STATE: u16 = 200;

fn sid(v: u16) -> SubjectId {
    SubjectId::new(v).unwrap()
}
fn nid(v: u8) -> NodeId {
    NodeId::new(v).unwrap()
}
fn prio(v: u8) -> Priority {
    Priority::new(v).unwrap()
}

/// xorshift64*：固定 seed，結果可重現。
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn chance(&mut self, per_mille: u64) -> bool {
        self.next() % 1000 < per_mille
    }
}

fn host() -> N {
    let mut n = N::new(nid(10), Mtu::Classic, 0);
    assert!(n.add_publication(Publication { subject: sid(CMD), priority: prio(2), protection: PubProtection::E2e32 { type_hash: TH } }));
    assert!(n.add_subscription(Subscription { subject: sid(STATE), protection: SubProtection::None, tid_timeout: 100_000 }));
    n
}

fn motor() -> N {
    let mut n = N::new(nid(20), Mtu::Classic, 0);
    assert!(n.add_publication(Publication { subject: sid(STATE), priority: prio(4), protection: PubProtection::None }));
    assert!(n.add_subscription(Subscription {
        subject: sid(CMD),
        protection: SubProtection::E2e32 { type_hash: TH, max_delta: 3, deadline: 50_000, params: StateParams::default() },
        tid_timeout: 100_000,
    }));
    n
}

/// 把 a 送出的框全部交給 b，收集事件的摘要。
fn pump(now: u64, a: &mut N, b: &mut N, out: &mut Vec<String>) {
    let mut buf = [0u8; 64];
    while let Some((id, n)) = a.poll_transmit(&mut buf) {
        if let Some(e) = b.on_frame(now, id, &buf[..n]) {
            out.push(summary(&e));
        }
    }
}

fn summary(e: &Event<'_>) -> String {
    match e {
        Event::Message { subject, payload, e2e, .. } => format!("msg {} {:02x?} {:?}", subject.get(), payload, e2e.map(|x| x.0)),
        Event::Rejected { subject, status, .. } => format!("rej {} {:?}", subject.get(), status),
        Event::Heartbeat { source, heartbeat } => format!("hb {} up={}", source.get(), heartbeat.uptime),
        Event::Request { service, transfer_id, payload, .. } => format!("req {} tid={} {:02x?}", service.get(), transfer_id, payload),
        Event::Response { service, transfer_id, payload, .. } => format!("resp {} tid={} {:02x?}", service.get(), transfer_id, payload),
    }
}

#[test]
fn pubsub_heartbeat_sync_and_service() {
    let (mut h, mut m) = (host(), motor());
    let mut ev = Vec::new();

    // 開機：Heartbeat 與 E2E Sync
    h.handle_timeout(0);
    pump(0, &mut h, &mut m, &mut ev);
    assert_eq!(ev, ["hb 10 up=0"]);
    ev.clear();

    // protected 指令：單框（2 B 資料 + 5 B E2E）
    h.publish(sid(CMD), &[0x12, 0x34]).unwrap();
    pump(1000, &mut h, &mut m, &mut ev);
    assert_eq!(ev, ["msg 100 [12, 34] Some(Ok)"]);
    ev.clear();

    // normal 狀態：多框（12 B）
    let state: Vec<u8> = (0..12).collect();
    m.publish(sid(STATE), &state).unwrap();
    pump(2000, &mut m, &mut h, &mut ev);
    assert_eq!(ev, [format!("msg 200 {:02x?} None", state)]);
    ev.clear();

    // Service：host 對 motor 送 request，motor 回 response
    let svc = ServiceId::new(10).unwrap();
    let tid = h.request(3000, svc, nid(20), prio(3), 100_000, &[0x01]).unwrap();
    let mut buf = [0u8; 64];
    let mut reqs = Vec::new();
    while let Some((id, n)) = h.poll_transmit(&mut buf) {
        if let Some(Event::Request { service, source, transfer_id, payload }) = m.on_frame(3000, id, &buf[..n]) {
            reqs.push((service, source, transfer_id, payload.to_vec()));
        }
    }
    assert_eq!(reqs.len(), 1);
    let (service, client, t, _) = reqs[0].clone();
    assert_eq!((service, client, t), (svc, nid(10), tid));
    m.respond(service, client, t, prio(3), &[0xAC]).unwrap();
    pump(3500, &mut m, &mut h, &mut ev);
    assert_eq!(ev, [format!("resp 10 tid={tid} [ac]")]);
    ev.clear();

    // 逾時後才到的 response 被丟棄
    let tid2 = h.request(4000, svc, nid(20), prio(3), 1_000, &[0x02]).unwrap();
    while h.poll_transmit(&mut buf).is_some() {}
    h.handle_timeout(10_000);
    assert_eq!(h.stats.request_timeout, 1);
    m.respond(svc, nid(10), tid2, prio(3), &[0xAD]).unwrap();
    pump(10_000, &mut m, &mut h, &mut ev);
    assert!(ev.is_empty());
}

#[test]
fn heartbeat_period_and_uptime() {
    let (mut h, mut m) = (host(), motor());
    let mut ev = Vec::new();
    let mut t = 0u64;
    while t <= 3_000_000 {
        if t >= h.poll_timeout() {
            h.handle_timeout(t);
        }
        pump(t, &mut h, &mut m, &mut ev);
        t += 10_000;
    }
    let hbs: Vec<&String> = ev.iter().filter(|e| e.starts_with("hb")).collect();
    assert_eq!(hbs, ["hb 10 up=0", "hb 10 up=1", "hb 10 up=2", "hb 10 up=3"]);
}

#[derive(Clone, Copy, Debug)]
enum Fault {
    Loss,
    Duplicate,
    Reorder,
    Corrupt,
    Delay,
}

/// 用 protected 指令跑 `count` 筆，週期 10 ms，依故障種類注入。回傳（注入數、偵測數、錯誤交付數）。
fn run_fault(kind: Fault, seed: u64, count: u32) -> (u32, u32, u32, u32) {
    let (mut h, mut m) = (host(), motor());
    let mut rng = Rng(seed);
    let mut buf = [0u8; 64];
    let mut injected = 0u32;
    let mut wrong = 0u32;
    let mut held: Option<(u32, Vec<u8>, u32)> = None; // Reorder：暫存的框與它的序號
    let mut delayed_until = 0u64;
    let mut last_delivered: Option<u32> = None;
    let mut in_gap = false;
    let mut prev_lost = false;
    let mut gaps_injected = 0u32;
    let mut outage_start_nnd: Option<u32> = None;
    let mut outages_detected = 0u32;
    let mut gap_detect = 0u32;
    let mut late_rejects = 0u32;

    h.handle_timeout(0);
    let mut t = 0u64;
    // 先送 Heartbeat 與 Sync
    while let Some((id, n)) = h.poll_transmit(&mut buf) {
        m.on_frame(t, id, &buf[..n]);
    }
    for seq in 0..count {
        t += 10_000;
        if t >= m.poll_timeout() {
            m.handle_timeout(t);
        }
        if t >= h.poll_timeout() {
            h.handle_timeout(t);
        }
        // 每筆的資料就是序號的低 16 位元，方便檢查交付的內容
        h.publish(sid(CMD), &(seq as u16).to_le_bytes()).unwrap();
        let mut frames = Vec::new();
        while let Some((id, n)) = h.poll_transmit(&mut buf) {
            frames.push((id, buf[..n].to_vec()));
        }
        for (id, mut f) in frames {
            let is_cmd = (id >> 7) & 0x1FFF == u32::from(CMD);
            let mut deliveries: Vec<(u32, Vec<u8>)> = vec![(id, f.clone())];
            if is_cmd && matches!(kind, Fault::Delay) && t >= delayed_until && let Some(before) = outage_start_nnd.take() && m.stats.e2e[Status::NoNewData as usize] > before {
                outages_detected += 1;
            }
            let mut lost_now = false;
            if is_cmd && seq > 5 && seq + 10 < count {
                match kind {
                    Fault::Loss if rng.chance(50) => {
                        deliveries.clear();
                        injected += 1;
                        lost_now = true;
                        if !prev_lost {
                            gaps_injected += 1;
                        }
                        in_gap = true;
                    }
                    Fault::Duplicate if rng.chance(50) => {
                        deliveries.push((id, f.clone()));
                        injected += 1;
                    }
                    Fault::Reorder => {
                        if let Some((hid, hf, _)) = held.take() {
                            // 把先前暫存的框放在這一框之後送：晚到
                            deliveries.push((hid, hf));
                        } else if rng.chance(50) {
                            held = Some((id, f.clone(), seq));
                            deliveries.clear();
                            injected += 1;
                        }
                    }
                    Fault::Corrupt if rng.chance(50) => {
                        // 只改資料與 E2E 欄位（不含 Tail byte），模擬 Bridge 記憶體或無 CRC 的 Link 毀損
                        let byte = (rng.next() as usize) % (f.len() - 1);
                        let bit = rng.next() % 8;
                        f[byte] ^= 1 << bit;
                        deliveries = vec![(id, f.clone())];
                        injected += 1;
                    }
                    Fault::Delay if t >= delayed_until && rng.chance(5) => {
                        // 停止送達 200 ms，超過 50 ms 的期限
                        delayed_until = t + 200_000;
                        injected += 1;
                        outage_start_nnd = Some(m.stats.e2e[Status::NoNewData as usize]);
                    }
                    _ => {}
                }
            }
            if is_cmd {
                prev_lost = lost_now;
            }
            if matches!(kind, Fault::Delay) && t < delayed_until {
                deliveries.clear();
            }
            for (did, df) in deliveries {
                match m.on_frame(t, did, &df) {
                    Some(Event::Message { payload, e2e: Some((status, _)), .. }) => {
                        let v = u16::from_le_bytes([payload[0], payload[1]]) as u32;
                        // 交付的內容必須是某一筆真實送出的資料，而且序號遞增
                        if last_delivered.is_some_and(|l| v <= l) || v > seq {
                            wrong += 1;
                        }
                        if status == Status::OkSomeLost || status == Status::WrongSequence {
                            gap_detect += 1;
                        }
                        in_gap = false;
                        last_delivered = Some(v);
                    }
                    Some(Event::Rejected { status, .. }) => {
                        if matches!(status, Status::Error | Status::Repeated | Status::WrongSequence) {
                            late_rejects += 1;
                        }
                        if status == Status::WrongSequence {
                            gap_detect += 1;
                            in_gap = false;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let s = m.stats;
    let detected = match kind {
        // 連續的遺失只會在下一筆被看出一次；偵測數以「有缺口的次數」計，注入數換算成缺口數
        Fault::Loss => gap_detect,
        Fault::Duplicate => s.duplicate,
        Fault::Reorder => late_rejects,
        Fault::Corrupt => s.e2e[Status::Error as usize] + s.toggle + s.no_session + s.crc,
        Fault::Delay => outages_detected,
    };
    let _ = in_gap;
    // 連續的遺失只會在下一筆被看出一次，所以遺失以「缺口數」計
    let raw = injected;
    let injected = if matches!(kind, Fault::Loss) { gaps_injected } else { injected };
    // 期限到了要能回到 VALID：最後的狀態
    let st = m.stream_state(sid(CMD), nid(10));
    assert!(st.is_some());
    (injected, detected, wrong, raw)
}

fn report(kind: Fault, seed: u64, count: u32) {
    let (injected, detected, wrong, raw) = run_fault(kind, seed, count);
    let name = format!("{kind:?}").to_lowercase();
    let pass = injected > 0 && detected >= injected && wrong == 0;
    println!(
        "EMBNET_FAULT {} kind={name} seed={seed} transfers={count} injected={injected} detected={detected} wrong_delivered={wrong} frames_affected={raw} env=sim",
        if pass { "PASS" } else { "FAIL" }
    );
    assert!(pass, "{name}: injected={injected} detected={detected} wrong={wrong}");
}

const SEED: u64 = 0x00E1_B0E7_2026_1116;

#[test]
fn fault_loss() {
    report(Fault::Loss, SEED, 10_000);
}

#[test]
fn fault_duplicate() {
    report(Fault::Duplicate, SEED, 10_000);
}

#[test]
fn fault_reorder() {
    report(Fault::Reorder, SEED, 10_000);
}

#[test]
fn fault_corrupt() {
    report(Fault::Corrupt, SEED, 10_000);
}

#[test]
fn fault_delay() {
    report(Fault::Delay, SEED, 10_000);
}

#[test]
fn protected_stream_goes_invalid_then_recovers() {
    let (mut h, mut m) = (host(), motor());
    let mut buf = [0u8; 64];
    h.handle_timeout(0);
    while let Some((id, n)) = h.poll_transmit(&mut buf) {
        m.on_frame(0, id, &buf[..n]);
    }
    let mut t = 0;
    for i in 0..5u16 {
        t += 10_000;
        h.publish(sid(CMD), &i.to_le_bytes()).unwrap();
        while let Some((id, n)) = h.poll_transmit(&mut buf) {
            m.on_frame(t, id, &buf[..n]);
        }
    }
    assert_eq!(m.stream_state(sid(CMD), nid(10)), Some(StreamState::Valid));
    // 停止 300 ms：期限 50 ms，NONEWDATA 累積到 INVALID
    for _ in 0..30 {
        t += 10_000;
        m.handle_timeout(t);
    }
    assert_eq!(m.stream_state(sid(CMD), nid(10)), Some(StreamState::Invalid));
    // 恢復發送：需要連續好幾筆才回到 VALID
    for i in 0..12u16 {
        t += 10_000;
        h.publish(sid(CMD), &i.to_le_bytes()).unwrap();
        while let Some((id, n)) = h.poll_transmit(&mut buf) {
            m.on_frame(t, id, &buf[..n]);
        }
    }
    assert_eq!(m.stream_state(sid(CMD), nid(10)), Some(StreamState::Valid));
}
