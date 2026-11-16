//! embnet 設定檔的讀取與建置期檢查，對應 `docs/spec/07-config.md`。
//!
//! 檢查編號 C1 到 C9 與規格第三節一致。延遲上限的算法見規格的「C7、C8 的算法」。

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use embnet_core::can::{Mtu, Segmenter};
use serde::Deserialize;

/// 設定檔的根。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// 格式版本，目前為 1。
    pub version: u32,
    /// Link。
    #[serde(default, rename = "link")]
    pub links: Vec<Link>,
    /// Node。
    #[serde(default, rename = "node")]
    pub nodes: Vec<Node>,
    /// 型別。
    #[serde(default, rename = "type")]
    pub types: Vec<Type>,
    /// Subject。
    #[serde(default, rename = "subject")]
    pub subjects: Vec<Subject>,
    /// Service。
    #[serde(default, rename = "service")]
    pub services: Vec<Service>,
    /// 轉送規則。
    #[serde(default, rename = "route")]
    pub routes: Vec<Route>,
}

/// Link 的種類。
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LinkKind {
    /// classic CAN。
    Can,
    /// CAN FD。
    CanFd,
    /// UART，COBS 分界。
    Uart,
    /// UDP。
    Udp,
}

/// 一條 Link。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    /// 名稱。
    pub name: String,
    /// 種類。
    pub kind: LinkKind,
    /// bit/s 或 baud。
    pub bitrate: u64,
    /// Trust zone。
    pub zone: String,
    /// 使用率上限。
    #[serde(default = "default_util")]
    pub util_limit: f64,
    /// UART 的最大 payload。
    pub max_payload: Option<usize>,
}

fn default_util() -> f64 {
    0.5
}

/// 一個 Node。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    /// 名稱。
    pub name: String,
    /// Node-ID。
    pub id: u32,
    /// 接上的 Link。
    pub links: Vec<String>,
    /// 是否為 Bridge。
    #[serde(default)]
    pub bridge: bool,
    /// Bridge 的處理時間上限，微秒。
    pub latency_us: Option<u64>,
}

/// 型別。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Type {
    /// 名稱。
    pub name: String,
    /// major 版本。
    pub major: u32,
    /// 序列化後的最大長度。
    pub max_len: usize,
    /// 欄位描述。
    #[serde(default)]
    pub fields: String,
}

/// Subject 的等級。
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Class {
    /// 一般。
    Normal,
    /// 錯了會造成風險。
    Protected,
}

/// 保護方式。
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Protection {
    /// CRC-32/AUTOSAR。
    E2e32,
    /// 32 位元 tag。
    Mac32,
    /// 64 位元 tag。
    Mac64,
}

impl Protection {
    /// 開銷位元組數。
    pub fn overhead(self) -> usize {
        match self {
            Protection::E2e32 | Protection::Mac32 => 5,
            Protection::Mac64 => 9,
        }
    }
}

/// 一個 Subject。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subject {
    /// 名稱。
    pub name: String,
    /// Subject-ID。
    pub id: u32,
    /// 型別名稱。
    #[serde(rename = "type")]
    pub type_name: String,
    /// 優先等級。
    pub priority: u8,
    /// 發佈者。
    pub publisher: String,
    /// 週期，微秒。
    pub period_us: u64,
    /// 抖動，微秒。
    #[serde(default)]
    pub jitter_us: u64,
    /// 等級。
    pub class: Class,
    /// 保護方式。
    pub protection: Option<Protection>,
    /// 金鑰編號。
    pub key: Option<u32>,
    /// 期限，微秒。
    pub deadline_us: Option<u64>,
    /// 容許的跳號。
    #[serde(default = "default_max_delta")]
    pub max_delta: u8,
    /// 丟棄策略。
    #[serde(default = "default_drop")]
    pub drop: String,
    /// Transfer-ID 逾時，微秒。
    #[serde(default = "default_tid_timeout")]
    pub tid_timeout_us: u64,
}

fn default_max_delta() -> u8 {
    3
}
fn default_drop() -> String {
    "oldest".into()
}
fn default_tid_timeout() -> u64 {
    1_000_000
}

/// 一個 Service。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Service {
    /// 名稱。
    pub name: String,
    /// Service-ID。
    pub id: u32,
    /// request 型別。
    pub request_type: String,
    /// response 型別。
    pub response_type: String,
    /// server。
    pub server: String,
    /// 優先等級。
    pub priority: u8,
    /// 逾時，微秒。
    #[serde(default = "default_tid_timeout")]
    pub timeout_us: u64,
}

/// 一條轉送規則。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    /// Subject 名稱。
    pub subject: Option<String>,
    /// Service 名稱。
    pub service: Option<String>,
    /// Bridge 的 Node 名稱。
    pub bridge: String,
    /// 入口 Link。
    pub from: String,
    /// 出口 Link。
    pub to: String,
}

/// 一條違規。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// 檢查編號，例如 "C6"。
    pub check: &'static str,
    /// 說明。
    pub message: String,
}

/// 一個 Subject 在一條 Link 上的計算結果。
#[derive(Debug, Clone)]
pub struct Flow {
    /// Subject 名稱。
    pub subject: String,
    /// Link 名稱。
    pub link: String,
    /// 優先等級。
    pub priority: u8,
    /// 每筆 Transfer 的位元數。
    pub bits: f64,
    /// 最長單框的位元數（非搶占阻擋用）。
    pub max_frame_bits: f64,
    /// 長期速率，bit/s。
    pub r: f64,
    /// 突發量，位元。
    pub b: f64,
    /// 這條 Link 上的延遲上限，微秒。
    pub bound_us: f64,
    /// 從發佈者到這條 Link 的累計延遲上限，微秒。
    pub path_bound_us: f64,
}

/// 檢查的結果。
#[derive(Debug, Default)]
pub struct Report {
    /// 違規。
    pub violations: Vec<Violation>,
    /// 執行的檢查數。
    pub checks: usize,
    /// 各 Link 的使用率。
    pub utilization: BTreeMap<String, f64>,
    /// 各 Subject 在各 Link 上的結果。
    pub flows: Vec<Flow>,
}

impl Report {
    fn fail(&mut self, check: &'static str, message: String) {
        self.violations.push(Violation { check, message });
    }

    /// 是否全部通過。
    pub fn passed(&self) -> bool {
        self.violations.is_empty()
    }

    /// 最大的使用率。
    pub fn max_util(&self) -> f64 {
        self.utilization.values().copied().fold(0.0, f64::max)
    }

    /// 最大的累計延遲上限。
    pub fn worst_bound_us(&self) -> f64 {
        self.flows.iter().map(|f| f.path_bound_us).fold(0.0, f64::max)
    }
}

/// Type hash：`<name>:<major>:<max_len>:<fields 正規化>` 的 CRC-64/XZ（規格第二節）。
pub fn type_hash(t: &Type) -> u64 {
    let collapsed = t.fields.split_whitespace().collect::<Vec<_>>().join(" ");
    let norm = collapsed.split(';').map(str::trim).collect::<Vec<_>>().join(";");
    let canon = format!("{}:{}:{}:{}", t.name, t.major, t.max_len, norm);
    embnet_core::crc::crc64_xz(canon.as_bytes())
}

/// 解析 TOML。
pub fn parse(text: &str) -> Result<Config, String> {
    toml::from_str(text).map_err(|e| e.to_string())
}

/// 一筆 Transfer 在某種 Link 上的位元數，以及最長單框的位元數（規格「C7、C8 的算法」）。
pub fn transfer_bits(kind: LinkKind, len: usize) -> (f64, f64) {
    match kind {
        LinkKind::Can | LinkKind::CanFd => {
            let mtu = if kind == LinkKind::Can { Mtu::Classic } else { Mtu::Fd };
            let payload = vec![0u8; len];
            let mut seg = Segmenter::new(&payload, mtu, 0);
            let mut buf = [0u8; 64];
            let mut total = 0.0;
            let mut max = 0.0f64;
            while let Some(n) = seg.next_frame(&mut buf) {
                // 延伸框、最壞填充：80 + 10n 位元（docs/10-buses/01 第四節）
                let bits = 80.0 + 10.0 * n as f64;
                total += bits;
                max = max.max(bits);
            }
            (total, max)
        }
        LinkKind::Uart => {
            let d = len + embnet_core::datagram::OVERHEAD;
            let bytes = d + d.div_ceil(254) + 1;
            let bits = 10.0 * bytes as f64;
            (bits, bits)
        }
        LinkKind::Udp => {
            let bytes = (len + embnet_core::datagram::OVERHEAD + 8 + 20 + 38).max(84);
            let bits = 8.0 * bytes as f64;
            (bits, bits)
        }
    }
}

/// 一個 Subject 出現在哪些 Link：值為 None 表示發佈者直接接在這條 Link 上，否則是（入口 Link、Bridge）。
type Presence = BTreeMap<String, Option<(String, String)>>;

/// 執行 C1 到 C9。
pub fn check(cfg: &Config) -> Report {
    let mut rep = Report::default();
    let links: BTreeMap<&str, &Link> = cfg.links.iter().map(|l| (l.name.as_str(), l)).collect();
    let nodes: BTreeMap<&str, &Node> = cfg.nodes.iter().map(|n| (n.name.as_str(), n)).collect();
    let types: BTreeMap<&str, &Type> = cfg.types.iter().map(|t| (t.name.as_str(), t)).collect();

    // C1：名稱與 ID
    rep.checks += 1;
    if cfg.version != 1 {
        rep.fail("C1", format!("version 是 {}，只支援 1", cfg.version));
    }
    let mut names = BTreeSet::new();
    let all_names = cfg.links.iter().map(|x| ("link", &x.name))
        .chain(cfg.nodes.iter().map(|x| ("node", &x.name)))
        .chain(cfg.types.iter().map(|x| ("type", &x.name)))
        .chain(cfg.subjects.iter().map(|x| ("subject", &x.name)))
        .chain(cfg.services.iter().map(|x| ("service", &x.name)));
    for (kind, n) in all_names {
        if !names.insert((kind, n.clone())) {
            rep.fail("C1", format!("{kind} 名稱重複：{n}"));
        }
    }
    let mut ids = BTreeSet::new();
    for n in &cfg.nodes {
        if n.id > 127 {
            rep.fail("C1", format!("Node {} 的 id {} 超出 0 到 127", n.name, n.id));
        }
        if !ids.insert(n.id) {
            rep.fail("C1", format!("Node-ID {} 重複", n.id));
        }
    }
    let mut ids = BTreeSet::new();
    for s in &cfg.subjects {
        if s.id > 7999 {
            rep.fail("C1", format!("Subject {} 的 id {} 超出應用範圍 0 到 7999", s.name, s.id));
        }
        if !ids.insert(s.id) {
            rep.fail("C1", format!("Subject-ID {} 重複", s.id));
        }
        if s.priority > 7 {
            rep.fail("C1", format!("Subject {} 的 priority {} 超出 0 到 7", s.name, s.priority));
        }
    }
    let mut ids = BTreeSet::new();
    for s in &cfg.services {
        if s.id > 499 {
            rep.fail("C1", format!("Service {} 的 id {} 超出應用範圍 0 到 499", s.name, s.id));
        }
        if !ids.insert(s.id) {
            rep.fail("C1", format!("Service-ID {} 重複", s.id));
        }
    }

    // C2：引用
    rep.checks += 1;
    for n in &cfg.nodes {
        for l in &n.links {
            if !links.contains_key(l.as_str()) {
                rep.fail("C2", format!("Node {} 引用不存在的 Link {l}", n.name));
            }
        }
        if n.bridge && n.latency_us.is_none() {
            rep.fail("C2", format!("Bridge {} 沒有 latency_us", n.name));
        }
    }
    for s in &cfg.subjects {
        if !types.contains_key(s.type_name.as_str()) {
            rep.fail("C2", format!("Subject {} 引用不存在的型別 {}", s.name, s.type_name));
        }
        if !nodes.contains_key(s.publisher.as_str()) {
            rep.fail("C2", format!("Subject {} 的發佈者 {} 不存在", s.name, s.publisher));
        }
    }
    for s in &cfg.services {
        for t in [&s.request_type, &s.response_type] {
            if !types.contains_key(t.as_str()) {
                rep.fail("C2", format!("Service {} 引用不存在的型別 {t}", s.name));
            }
        }
        if !nodes.contains_key(s.server.as_str()) {
            rep.fail("C2", format!("Service {} 的 server {} 不存在", s.name, s.server));
        }
    }

    // C3：protected 的必要欄位
    rep.checks += 1;
    for s in &cfg.subjects {
        if s.class == Class::Protected {
            match s.protection {
                None => rep.fail("C3", format!("protected Subject {} 沒有 protection", s.name)),
                Some(Protection::Mac32 | Protection::Mac64) if s.key.is_none() => {
                    rep.fail("C3", format!("Subject {} 用 MAC 但沒有 key", s.name))
                }
                _ => {}
            }
            if s.deadline_us.is_none() {
                rep.fail("C3", format!("protected Subject {} 沒有 deadline_us", s.name));
            }
        } else if s.protection.is_some() {
            rep.fail("C3", format!("normal Subject {} 不應設定 protection", s.name));
        }
        if s.drop != "oldest" && s.drop != "newest" {
            rep.fail("C3", format!("Subject {} 的 drop 必須是 oldest 或 newest", s.name));
        }
    }

    // C4：Transfer-ID 逾時
    rep.checks += 1;
    for s in &cfg.subjects {
        if s.tid_timeout_us >= s.period_us.saturating_mul(32) {
            rep.fail("C4", format!("Subject {} 的 tid_timeout_us {} 不小於 32 個週期", s.name, s.tid_timeout_us));
        }
        if s.period_us == 0 {
            rep.fail("C4", format!("Subject {} 的 period_us 是 0", s.name));
        }
    }

    // C5、C6：轉送與路徑
    rep.checks += 2;
    // 每個 Subject 出現在哪些 Link，以及從哪條 Link、經哪個 Bridge 進來
    let mut presence: BTreeMap<&str, Presence> = BTreeMap::new();
    for s in &cfg.subjects {
        let mut seen: BTreeMap<String, Option<(String, String)>> = BTreeMap::new();
        let mut queue = VecDeque::new();
        if let Some(p) = nodes.get(s.publisher.as_str()) {
            for l in &p.links {
                seen.insert(l.clone(), None);
                queue.push_back(l.clone());
            }
        }
        let routes: Vec<&Route> = cfg.routes.iter().filter(|r| r.subject.as_deref() == Some(s.name.as_str())).collect();
        let mut used = vec![false; routes.len()];
        while let Some(at) = queue.pop_front() {
            for (i, r) in routes.iter().enumerate() {
                if r.from != at {
                    continue;
                }
                if let Some(flag) = used.get_mut(i) {
                    *flag = true;
                }
                if seen.contains_key(&r.to) {
                    rep.fail("C6", format!("Subject {} 有兩條以上的路徑進入 Link {}（或形成迴路）", s.name, r.to));
                    continue;
                }
                seen.insert(r.to.clone(), Some((r.from.clone(), r.bridge.clone())));
                queue.push_back(r.to.clone());
            }
        }
        for (i, r) in routes.iter().enumerate() {
            if !used.get(i).copied().unwrap_or(false) {
                rep.fail("C5", format!("Subject {} 不會出現在 Link {}，轉送到 {} 的規則無效", s.name, r.from, r.to));
            }
        }
        presence.insert(s.name.as_str(), seen);
    }
    for r in &cfg.routes {
        match (r.subject.is_some(), r.service.is_some()) {
            (true, false) | (false, true) => {}
            _ => rep.fail("C5", format!("轉送規則 {} 到 {} 必須只指定 subject 或 service 其中一個", r.from, r.to)),
        }
        if let Some(sv) = &r.service
            && !cfg.services.iter().any(|x| &x.name == sv)
        {
            rep.fail("C5", format!("轉送規則引用不存在的 Service {sv}"));
        }
        if let Some(sj) = &r.subject
            && !cfg.subjects.iter().any(|x| &x.name == sj)
        {
            rep.fail("C5", format!("轉送規則引用不存在的 Subject {sj}"));
        }
        match nodes.get(r.bridge.as_str()) {
            Some(b) if b.bridge => {
                for l in [&r.from, &r.to] {
                    if !b.links.contains(l) {
                        rep.fail("C5", format!("Bridge {} 沒有接在 Link {l} 上", r.bridge));
                    }
                }
            }
            _ => rep.fail("C5", format!("轉送規則的 {} 不是 Bridge", r.bridge)),
        }
        if r.from == r.to {
            rep.fail("C5", format!("轉送規則的入口與出口都是 {}", r.from));
        }
    }

    // C7、C8、C9：流量、延遲、長度
    rep.checks += 3;
    let mut flows: Vec<Flow> = Vec::new();
    for s in &cfg.subjects {
        let Some(t) = types.get(s.type_name.as_str()) else { continue };
        let len = t.max_len + s.protection.map_or(0, Protection::overhead);
        let Some(seen) = presence.get(s.name.as_str()) else { continue };
        for lname in seen.keys() {
            let Some(l) = links.get(lname.as_str()) else { continue };
            if l.kind == LinkKind::Uart && l.max_payload.is_none_or(|m| m < len) {
                rep.fail("C9", format!("UART {} 的 max_payload 小於 Subject {} 的 {len} B", l.name, s.name));
            }
            if l.kind == LinkKind::Udp && len > 1460 {
                rep.fail("C9", format!("Subject {} 的 {len} B 超過 UDP 上限 1460 B", s.name));
            }
            let (bits, max_frame_bits) = transfer_bits(l.kind, len);
            let period = s.period_us.max(1) as f64 / 1e6;
            let r = bits / period;
            let b = bits * (s.jitter_us as f64 / s.period_us.max(1) as f64 + 1.0);
            flows.push(Flow {
                subject: s.name.clone(),
                link: lname.clone(),
                priority: s.priority,
                bits,
                max_frame_bits,
                r,
                b,
                bound_us: 0.0,
                path_bound_us: 0.0,
            });
        }
    }
    for l in &cfg.links {
        let on: Vec<usize> = (0..flows.len()).filter(|&i| flows.get(i).is_some_and(|f| f.link == l.name)).collect();
        let rate = l.bitrate as f64;
        let sum_r: f64 = on.iter().filter_map(|&i| flows.get(i)).map(|f| f.r).sum();
        let util = if rate > 0.0 { sum_r / rate } else { f64::INFINITY };
        rep.utilization.insert(l.name.clone(), util);
        if util > l.util_limit {
            rep.fail("C7", format!("Link {} 的使用率 {:.1}% 超過上限 {:.1}%", l.name, util * 100.0, l.util_limit * 100.0));
        }
        // 經由 Bridge 送進這條 Link 時，加上該 Bridge 的處理時間
        let bridge_t: f64 = presence
            .values()
            .filter_map(|seen| seen.get(&l.name))
            .filter_map(|via| via.as_ref())
            .filter_map(|(_, br)| nodes.get(br.as_str()).and_then(|n| n.latency_us))
            .map(|us| us as f64 / 1e6)
            .fold(0.0, f64::max);
        for &i in &on {
            let Some(f) = flows.get(i).cloned() else { continue };
            let lower_block = on
                .iter()
                .filter_map(|&j| flows.get(j))
                .filter(|g| g.priority > f.priority)
                .map(|g| g.max_frame_bits)
                .fold(0.0, f64::max);
            let t = lower_block / rate + bridge_t;
            let (r_h, b_h): (f64, f64) = on
                .iter()
                .filter_map(|&j| flows.get(j))
                .filter(|g| g.priority < f.priority)
                .fold((0.0, 0.0), |(r, b), g| (r + g.r, b + g.b));
            let b_p: f64 = on.iter().filter_map(|&j| flows.get(j)).filter(|g| g.priority == f.priority).map(|g| g.b).sum();
            let r_res = rate - r_h;
            let bound = if r_res <= 0.0 {
                rep.fail("C8", format!("Link {} 上優先等級 {} 的剩餘服務速率不大於 0", l.name, f.priority));
                f64::INFINITY
            } else {
                let t2 = t + (b_h + r_h * t) / r_res;
                t2 + b_p / r_res
            };
            if let Some(slot) = flows.get_mut(i) {
                slot.bound_us = bound * 1e6;
            }
        }
    }
    // 沿路徑累加：Link 的累計上限 = 前一條 Link 的累計上限 + 這條 Link 的上限
    for s in &cfg.subjects {
        let Some(seen) = presence.get(s.name.as_str()) else { continue };
        let mut order: Vec<String> = seen.iter().filter(|(_, v)| v.is_none()).map(|(k, _)| k.clone()).collect();
        let mut idx = 0;
        while let Some(at) = order.get(idx).cloned() {
            idx += 1;
            let prev = seen
                .get(&at)
                .and_then(|v| v.as_ref())
                .and_then(|(from, _)| flows.iter().find(|f| f.subject == s.name && &f.link == from))
                .map_or(0.0, |f| f.path_bound_us);
            if let Some(f) = flows.iter_mut().find(|f| f.subject == s.name && f.link == at) {
                f.path_bound_us = prev + f.bound_us;
            }
            for (k, v) in seen {
                if v.as_ref().is_some_and(|(from, _)| *from == at) {
                    order.push(k.clone());
                }
            }
        }
        if let (Class::Protected, Some(d)) = (s.class, s.deadline_us) {
            for f in flows.iter().filter(|f| f.subject == s.name) {
                if f.path_bound_us > d as f64 {
                    rep.fail(
                        "C8",
                        format!("Subject {} 到 Link {} 的延遲上限 {:.0} µs 超過期限 {d} µs", s.name, f.link, f.path_bound_us),
                    );
                }
            }
        }
    }
    rep.flows = flows;
    rep
}
