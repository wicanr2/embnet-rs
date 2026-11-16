//! Bridge 的離散事件模擬。拓撲與 `crates/embnet-config/tests/fixtures/good.toml` 相同：
//! host（UDP，eth0）─ bridge1 ─ motor（CAN，can1），imu（UART，uart0）─ bridge1。
//!
//! - 時間單位 1 µs。CAN 依 ID 仲裁，框長取最壞填充 80 + 10n 位元；UDP 與 UART 是全雙工的點對點通道。
//! - Bridge 收到框後經過 `latency_us`（設定檔的 200 µs）才處理，對應服務曲線的 T。
//! - 量每筆 Subject 從發佈到最終接收端交出的延遲，與 embnet-config 對同一份設定算出的上限比較。
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects,
    missing_docs
)]

use std::collections::{BTreeMap, VecDeque};

use embnet_bridge::{Bridge, DropPolicy, LinkKind, Port, Rule, TxOut};
use embnet_core::can::{Mtu, NodeId, Priority, ServiceId, SubjectId};
use embnet_core::datagram::{self, Header};
use embnet_core::e2e::{self, DataId, StateParams, Status};
use embnet_node::{Event, Node, PubProtection, Publication, SubProtection, Subscription};

const ETH: u8 = 0;
const CAN: u8 = 1;
const UART: u8 = 2;
const HOST: u8 = 10;
const MOTOR: u8 = 20;
const IMU: u8 = 30;
const SPEED: u16 = 100;
const STATE: u16 = 200;
const IMU_S: u16 = 300;
const SET_MODE: u16 = 10;

type Br = Bridge<3, 8, 64, 8, 128>;
type Motor = Node<4, 4, 4, 8, 64, 32, 4>;

fn sid(v: u16) -> SubjectId {
    SubjectId::new(v).unwrap()
}
fn nid(v: u8) -> NodeId {
    NodeId::new(v).unwrap()
}
fn prio(v: u8) -> Priority {
    Priority::new(v).unwrap()
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }
}

fn config() -> embnet_config::Config {
    embnet_config::parse(include_str!("../../embnet-config/tests/fixtures/good.toml")).unwrap()
}

fn type_hash(cfg: &embnet_config::Config, name: &str) -> u64 {
    embnet_config::type_hash(cfg.types.iter().find(|t| t.name == name).unwrap())
}

/// 依設定檔建出 Bridge 的規則。
fn bridge(cfg: &embnet_config::Config) -> Br {
    let mut b = Br::new([LinkKind::Datagram, LinkKind::Can(Mtu::Classic), LinkKind::Datagram]);
    let idx = |name: &str| ["eth0", "can1", "uart0"].iter().position(|x| *x == name).unwrap() as u8;
    let node_id = |name: &str| cfg.nodes.iter().find(|n| n.name == name).unwrap().id as u8;
    let mut syncs: Vec<(u8, u8, u8)> = Vec::new();
    for r in &cfg.routes {
        let rule = if let Some(s) = &r.subject {
            let s = cfg.subjects.iter().find(|x| &x.name == s).unwrap();
            if s.class == embnet_config::Class::Protected {
                let key = (node_id(&s.publisher), idx(&r.from), idx(&r.to));
                if !syncs.contains(&key) {
                    syncs.push(key);
                }
            }
            let burst = (s.jitter_us / s.period_us.max(1) + 2) as u32;
            Rule {
                port: Port::Subject(sid(s.id as u16)),
                source: nid(node_id(&s.publisher)),
                server: None,
                ingress: idx(&r.from),
                egress: idx(&r.to),
                period_us: s.period_us,
                burst,
                drop: if s.drop == "newest" { DropPolicy::Newest } else { DropPolicy::Oldest },
                deadline_us: s.deadline_us,
            }
        } else {
            let sv = cfg.services.iter().find(|x| Some(&x.name) == r.service.as_ref()).unwrap();
            Rule {
                port: Port::Service(ServiceId::new(sv.id as u16).unwrap()),
                source: nid(HOST),
                server: Some(nid(node_id(&sv.server))),
                ingress: idx(&r.from),
                egress: idx(&r.to),
                period_us: 10_000,
                burst: 4,
                drop: DropPolicy::Newest,
                deadline_us: Some(sv.timeout_us),
            }
        };
        assert!(b.add_rule(rule, 0));
    }
    // docs/spec/06-forwarding.md 第五節：protected Subject 的路徑自動附帶發佈者的 E2E Sync
    for (src, from, to) in syncs {
        let rule = Rule {
            port: Port::Subject(sid(e2e::SYNC_SUBJECT_ID)),
            source: nid(src),
            server: None,
            ingress: from,
            egress: to,
            period_us: 1_000_000,
            burst: 2,
            drop: DropPolicy::Oldest,
            deadline_us: None,
        };
        assert!(b.add_rule(rule, 0));
    }
    b
}

/// datagram 通道上一筆的位元數（與 embnet-config 的算法相同）。
fn dgram_bits(kind: &str, len: usize) -> u64 {
    match kind {
        "udp" => 8 * (len + 8 + 20 + 38).max(84) as u64,
        _ => 10 * (len + len.div_ceil(254) + 1) as u64,
    }
}

/// 一個 datagram 端點：UDP 主機或 UART 上的 IMU。用 embnet-core 直接編解碼。
struct Endpoint {
    id: u8,
    tids: BTreeMap<(bool, u16, u8), u8>,
    seq: BTreeMap<u16, u32>,
}

impl Endpoint {
    fn new(id: u8) -> Self {
        Self { id, tids: BTreeMap::new(), seq: BTreeMap::new() }
    }

    fn subject(&mut self, prio_v: u8, subject: u16, payload: &[u8], protect: Option<u64>) -> Vec<u8> {
        let tid = self.tids.entry((false, subject, 0)).or_insert(0);
        let t = *tid;
        *tid = tid.wrapping_add(1);
        let mut body = [0u8; 128];
        let p: &[u8] = if let Some(th) = protect {
            let s = self.seq.entry(subject).or_insert(0);
            let did = DataId { subject_id: subject, source: self.id, type_hash: th };
            let n = e2e::encode(&did, *s, payload, &mut body).unwrap();
            *s += 1;
            &body[..n]
        } else {
            payload
        };
        let h = Header { priority: prio_v, service: false, request: false, port: subject, source: self.id, destination: 0xFF, transfer_id: t };
        let mut out = [0u8; 256];
        let n = datagram::encode(&h, p, &mut out).unwrap();
        out[..n].to_vec()
    }

    /// 這個端點所有 protected Subject 的 E2E Sync。
    fn sync(&mut self) -> Vec<u8> {
        let entries: Vec<(u16, u32)> = self.seq.iter().map(|(&s, &q)| (s, q)).collect();
        let mut body = [0u8; 128];
        let n = e2e::sync_encode(self.id, &entries, &mut body).unwrap();
        let tid = self.tids.entry((false, e2e::SYNC_SUBJECT_ID, 0)).or_insert(0);
        let t = *tid;
        *tid = tid.wrapping_add(1);
        let h = Header { priority: 4, service: false, request: false, port: e2e::SYNC_SUBJECT_ID, source: self.id, destination: 0xFF, transfer_id: t };
        let mut out = [0u8; 256];
        let m = datagram::encode(&h, &body[..n], &mut out).unwrap();
        out[..m].to_vec()
    }

    fn request(&mut self, service: u16, server: u8, payload: &[u8]) -> (Vec<u8>, u8) {
        let tid = self.tids.entry((true, service, server)).or_insert(0);
        let t = *tid;
        *tid = tid.wrapping_add(1);
        let h = Header { priority: 3, service: true, request: true, port: service, source: self.id, destination: server, transfer_id: t };
        let mut out = [0u8; 256];
        let n = datagram::encode(&h, payload, &mut out).unwrap();
        (out[..n].to_vec(), t)
    }
}

/// 一個方向的點對點通道。
#[derive(Default)]
struct Chan {
    busy_until: u64,
    queue: VecDeque<Vec<u8>>,
    in_flight: VecDeque<(u64, Vec<u8>)>,
}

struct Sim {
    t: u64,
    rng: Rng,
    br: Br,
    motor: Motor,
    host: Endpoint,
    imu: Endpoint,
    th_speed: u64,
    // CAN 匯流排
    can_busy: u64,
    can_slot_motor: Option<(u32, Vec<u8>)>,
    can_slot_bridge: Option<(u32, Vec<u8>)>,
    can_in_flight: Option<(u64, u32, Vec<u8>, bool)>, // (結束時間, id, data, 來自 bridge)
    // datagram 通道
    eth_up: Chan,
    eth_down: Chan,
    uart_up: Chan,
    uart_down: Chan,
    // 送進 Bridge、等待處理的
    br_in: VecDeque<(u64, u8, Option<u32>, Vec<u8>)>,
    latency: u64,
    // 量測
    sent: BTreeMap<(u16, u32), u64>,
    lat: BTreeMap<u16, Vec<u64>>,
    pending_req: BTreeMap<u8, u64>,
    resp_ok: u32,
    e2e_bad: u32,
}

impl Sim {
    fn new(seed: u64) -> Self {
        Self::from_cfg(seed, &config())
    }

    fn from_cfg(seed: u64, cfg: &embnet_config::Config) -> Self {
        let th_speed = type_hash(cfg, "demo.Speed");
        let mut motor = Motor::new(nid(MOTOR), Mtu::Classic, 0);
        motor.add_publication(Publication { subject: sid(STATE), priority: prio(4), protection: PubProtection::None });
        motor.add_subscription(Subscription {
            subject: sid(SPEED),
            protection: SubProtection::E2e32 { type_hash: th_speed, max_delta: 3, deadline: 50_000, params: StateParams::default() },
            tid_timeout: 100_000,
        });
        let lat_us = cfg.nodes.iter().find(|n| n.name == "bridge1").unwrap().latency_us.unwrap();
        Sim {
            t: 0,
            rng: Rng(seed),
            br: bridge(cfg),
            motor,
            host: Endpoint::new(HOST),
            imu: Endpoint::new(IMU),
            th_speed,
            can_busy: 0,
            can_slot_motor: None,
            can_slot_bridge: None,
            can_in_flight: None,
            eth_up: Chan::default(),
            eth_down: Chan::default(),
            uart_up: Chan::default(),
            uart_down: Chan::default(),
            br_in: VecDeque::new(),
            latency: lat_us,
            sent: BTreeMap::new(),
            lat: BTreeMap::new(),
            pending_req: BTreeMap::new(),
            resp_ok: 0,
            e2e_bad: 0,
        }
    }

    fn record(&mut self, subject: u16, tag: u32) {
        if let Some(t0) = self.sent.remove(&(subject, tag)) {
            self.lat.entry(subject).or_default().push(self.t - t0);
        }
    }

    fn host_rx(&mut self, d: &[u8]) {
        let (h, p) = datagram::decode(d).unwrap();
        if h.service && !h.request {
            if self.pending_req.remove(&h.transfer_id).is_some() && p == [0xAC] {
                self.resp_ok += 1;
            }
            return;
        }
        let tag = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
        self.record(h.port, tag);
    }

    fn step_chan(t: u64, ch: &mut Chan, bits_per: impl Fn(usize) -> u64, rate: u64) -> Option<Vec<u8>> {
        if ch.busy_until <= t
            && let Some(d) = ch.queue.pop_front()
        {
            let dur = (bits_per(d.len()) * 1_000_000).div_ceil(rate);
            ch.busy_until = t + dur;
            ch.in_flight.push_back((t + dur, d));
        }
        if ch.in_flight.front().is_some_and(|(end, _)| *end <= t) {
            return ch.in_flight.pop_front().map(|(_, d)| d);
        }
        None
    }

    fn tick(&mut self) {
        let t = self.t;
        // Bridge 處理到期的輸入
        while self.br_in.front().is_some_and(|(at, ..)| *at <= t) {
            let (_, link, id, data) = self.br_in.pop_front().unwrap();
            match id {
                Some(id) => self.br.on_can(link, t, id, &data),
                None => self.br.on_datagram(link, t, &data),
            }
        }
        // Bridge 的出口：datagram 通道
        let mut buf = [0u8; 256];
        for (link, ch) in [(ETH, &mut self.eth_down), (UART, &mut self.uart_down)] {
            if ch.queue.is_empty()
                && ch.busy_until <= t
                && let Some(TxOut::Datagram { len }) = self.br.poll_transmit(usize::from(link), t, &mut buf)
            {
                ch.queue.push_back(buf[..len].to_vec());
            }
        }
        // Node 與 Bridge 的 CAN 發送槽
        if self.can_slot_motor.is_none()
            && let Some((id, n)) = self.motor.poll_transmit(&mut buf)
        {
            self.can_slot_motor = Some((id, buf[..n].to_vec()));
        }
        if self.can_slot_bridge.is_none()
            && let Some(TxOut::Can { id, len }) = self.br.poll_transmit(usize::from(CAN), t, &mut buf)
        {
            self.can_slot_bridge = Some((id, buf[..len].to_vec()));
        }
        // CAN：結束的框送達，空閒時仲裁
        if let Some((end, id, data, from_bridge)) = self.can_in_flight.clone()
            && end <= t
        {
            self.can_in_flight = None;
            if from_bridge {
                match self.motor.on_frame(t, id, &data) {
                    Some(Event::Message { subject, payload, e2e, .. }) if subject.get() == SPEED => {
                        if !matches!(e2e, Some((Status::Ok | Status::OkSomeLost, _))) {
                            self.e2e_bad += 1;
                        }
                        let tag = u32::from_le_bytes([payload[0], payload[1], 0, 0]);
                        self.record(SPEED, tag);
                    }
                    Some(Event::Rejected { .. }) => self.e2e_bad += 1,
                    Some(Event::Request { service, source, transfer_id, .. }) => {
                        self.motor.respond(service, source, transfer_id, prio(3), &[0xAC]).unwrap();
                    }
                    _ => {}
                }
            } else {
                self.br_in.push_back((t + self.latency, CAN, Some(id), data));
            }
        }
        if self.can_in_flight.is_none() && self.can_busy <= t {
            let m = self.can_slot_motor.as_ref().map(|x| x.0);
            let b = self.can_slot_bridge.as_ref().map(|x| x.0);
            let pick_bridge = match (m, b) {
                (Some(a), Some(c)) => c < a,
                (None, Some(_)) => true,
                _ => false,
            };
            let slot = if pick_bridge { self.can_slot_bridge.take() } else { self.can_slot_motor.take() };
            if let Some((id, data)) = slot {
                let bits = 80 + 10 * data.len() as u64; // 1 Mbit/s：1 位元 = 1 µs
                self.can_busy = t + bits;
                self.can_in_flight = Some((t + bits, id, data, pick_bridge));
            }
        }
        // datagram 通道
        if let Some(d) = Self::step_chan(t, &mut self.eth_up, |n| dgram_bits("udp", n), 100_000_000) {
            self.br_in.push_back((t + self.latency, ETH, None, d));
        }
        if let Some(d) = Self::step_chan(t, &mut self.uart_up, |n| dgram_bits("uart", n), 115_200) {
            self.br_in.push_back((t + self.latency, UART, None, d));
        }
        if let Some(d) = Self::step_chan(t, &mut self.eth_down, |n| dgram_bits("udp", n), 100_000_000) {
            self.host_rx(&d);
        }
        if let Some(d) = Self::step_chan(t, &mut self.uart_down, |n| dgram_bits("uart", n), 115_200) {
            let _ = d; // imu 不訂閱任何東西
        }
        self.t += 1;
    }

    /// 跑 `dur_us` 微秒。發佈週期與抖動依設定檔；`speed_period` 可以覆寫來做負載測試。
    fn run(&mut self, dur_us: u64, speed_period: u64, speed_jitter: u64) {
        let mut next_speed = 1000u64;
        let mut speed_k = 0u64;
        let mut next_state = 3000u64;
        let mut next_imu = 5000u64;
        let mut next_req = 7000u64;
        let mut next_sync = 0u64;
        self.host.seq.entry(SPEED).or_insert(0);
        let mut tag = 0u32;
        let end = self.t + dur_us;
        self.motor.handle_timeout(0);
        while self.t < end {
            let t = self.t;
            if t >= self.motor.poll_timeout() {
                self.motor.handle_timeout(t);
            }
            if t >= next_sync {
                let d = self.host.sync();
                self.eth_up.queue.push_back(d);
                next_sync = t + 1_000_000;
            }
            if t >= next_speed {
                tag += 1;
                let payload = (tag as u16).to_le_bytes();
                let th = self.th_speed;
                let d = self.host.subject(2, SPEED, &payload, Some(th));
                self.sent.insert((SPEED, u32::from(tag as u16)), t);
                self.eth_up.queue.push_back(d);
                // 抖動依規格的定義：每筆落在自己的排定時刻 ± jitter/2，誤差不累積
                speed_k += 1;
                next_speed = 1000 + speed_k * speed_period - speed_jitter / 2 + self.rng.below(speed_jitter + 1);
            }
            if t >= next_state {
                tag += 1;
                let mut p = [0u8; 12];
                p[..4].copy_from_slice(&tag.to_le_bytes());
                self.sent.insert((STATE, tag), t);
                self.motor.publish(sid(STATE), &p).unwrap();
                next_state = t + 20_000;
            }
            if t >= next_imu {
                tag += 1;
                let mut p = [0u8; 24];
                p[..4].copy_from_slice(&tag.to_le_bytes());
                self.sent.insert((IMU_S, tag), t);
                let d = self.imu.subject(3, IMU_S, &p, None);
                self.uart_up.queue.push_back(d);
                next_imu = t + 10_000;
            }
            if t >= next_req {
                let (d, tid) = self.host.request(SET_MODE, MOTOR, &[0x01]);
                self.pending_req.insert(tid, t);
                self.eth_up.queue.push_back(d);
                next_req = t + 100_000;
            }
            self.tick();
        }
    }
}

fn pct(v: &mut [u64], p: f64) -> u64 {
    v.sort_unstable();
    v[((p * v.len() as f64).ceil() as usize).saturating_sub(1).min(v.len() - 1)]
}

#[test]
fn end_to_end_within_config_bounds() {
    let cfg = config();
    let rep = embnet_config::check(&cfg);
    assert!(rep.passed());
    let bound = |subject: &str, link: &str| {
        rep.flows.iter().find(|f| f.subject == subject && f.link == link).unwrap().path_bound_us
    };
    let mut sim = Sim::new(0x5EED_0001);
    let dur = 5_000_000; // 5 秒
    sim.run(dur, 10_000, 1_000);
    let s = sim.br.stats;
    assert_eq!(s.no_route, 0, "{s:?}");
    assert_eq!(s.queue_dropped, 0, "{s:?}");
    assert_eq!(s.rate_limited, 0, "{s:?}");
    assert_eq!(sim.e2e_bad, 0, "E2E 欄位必須原樣通過 Bridge");
    // Service：host 用 datagram 的 Transfer-ID（模 256），經 Bridge 轉成 CAN 的（模 32）再回來；跑超過 32 次以涵蓋繞圈
    assert!(sim.resp_ok >= 45, "response {}", sim.resp_ok);
    for (subject, sname, link) in [(SPEED, "speed_cmd", "can1"), (STATE, "motor_state", "eth0"), (IMU_S, "imu", "eth0")] {
        let mut v = sim.lat.remove(&subject).unwrap_or_default();
        let n = v.len();
        assert!(n > 200, "{sname} 只收到 {n} 筆");
        let max = *v.iter().max().unwrap();
        let p99 = pct(&mut v, 0.99);
        let b = bound(sname, link);
        let ok = (max as f64) <= b;
        println!(
            "EMBNET_HOP {} in=publisher out={link} subject={sname} env=sim samples={n} p50_us={} p99_us={p99} max_us={max} bound_us={b:.0}",
            if ok { "PASS" } else { "FAIL" },
            pct(&mut v, 0.5)
        );
        assert!(ok, "{sname}: max {max} > bound {b:.0}");
    }
    println!("EMBNET_SERVICE PASS via=bridge requests_answered={} tid_modulo=256->32->256 env=sim", sim.resp_ok);
}

#[test]
fn unrouted_and_rate_limited() {
    let mut sim = Sim::new(7);
    // 沒有列入轉送表的 Subject：丟棄並計數
    let d = sim.host.subject(2, 555, &[1, 2, 3, 4], None);
    sim.eth_up.queue.push_back(d);
    // speed_cmd 以 10 倍速率洪泛：Bridge 入口限流
    sim.run(500_000, 1_000, 0);
    let s = sim.br.stats;
    assert_eq!(s.no_route, 1, "{s:?}");
    assert!(s.rate_limited > 300, "{s:?}");
    println!("EMBNET_ZONE PASS attempts=1 forwarded=0 env=sim");
    println!("EMBNET_RATELIMIT PASS offered_x10 rate_limited={} forwarded={} env=sim", s.rate_limited, s.forwarded);
}

/// 負載測試（`003` 第 1.6 節）：在 can1 上加一個低優先序的大量 Subject，逐步提高速率。
#[test]
#[ignore = "量測用，要 release 建置"]
fn load_knee() {
    let base = include_str!("../../embnet-config/tests/fixtures/good.toml");
    for pct_util in [10u64, 30, 50, 70, 80, 90, 95, 100] {
        // speed_cmd 每筆在 can1 上 2 框共 290 位元；調整週期讓它單獨占 pct_util% 的匯流排。
        // 設定檔跟著改，入口限流才會放行，量到的是出口的排隊而不是限流。
        let period = (290 * 100 / pct_util).max(1);
        let text = base
            .replacen("period_us = 10_000\njitter_us = 1_000", &format!("period_us = {period}\njitter_us = {}", period / 2), 1)
            .replacen("deadline_us = 50_000", "deadline_us = 1_000_000", 1)
            .replacen("tid_timeout_us = 100_000\n\n[[subject]]\nname = \"motor_state\"", &format!("tid_timeout_us = {}\n\n[[subject]]\nname = \"motor_state\"", period * 16), 1);
        let cfg = embnet_config::parse(&text).unwrap();
        let rep = embnet_config::check(&cfg);
        let bound = rep.flows.iter().find(|f| f.subject == "speed_cmd" && f.link == "can1").map_or(f64::INFINITY, |f| f.path_bound_us);
        let util = rep.utilization.get("can1").copied().unwrap_or(0.0) * 100.0;
        let mut sim = Sim::from_cfg(42, &cfg);
        sim.run(2_000_000, period, period / 2);
        let mut ms = sim.lat.remove(&STATE).unwrap_or_default();
        let ms_n = ms.len();
        let ms_p99 = if ms_n > 0 { pct(&mut ms, 0.99) } else { 0 };
        let ms_max = ms.iter().copied().max().unwrap_or(0);
        let ms_bound = rep.flows.iter().find(|f| f.subject == "motor_state" && f.link == "eth0").map_or(f64::INFINITY, |f| f.path_bound_us);
        let mut v = sim.lat.remove(&SPEED).unwrap_or_default();
        let n = v.len();
        let p99 = if n > 0 { pct(&mut v, 0.99) } else { 0 };
        let max = v.iter().copied().max().unwrap_or(0);
        let s = sim.br.stats;
        println!(
            "EMBNET_LOAD in=eth0 out=can1 subject=speed_cmd offered_util={pct_util}% can1_util={util:.0}% samples={n} p99_us={p99} max_us={max} bound_us={bound:.0} motor_state_samples={ms_n} motor_state_p99_us={ms_p99} motor_state_max_us={ms_max} motor_state_bound_us={ms_bound:.0} rate_limited={} queue_dropped={} deadline_dropped={} env=sim",
            s.rate_limited, s.queue_dropped, s.deadline_dropped
        );
    }
}
