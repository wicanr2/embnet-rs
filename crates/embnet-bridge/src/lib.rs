//! embnet 的 Bridge，對應 `docs/spec/06-forwarding.md`。
//!
//! sans-IO：外層收到框就呼叫 [`Bridge::on_can`] 或 [`Bridge::on_datagram`]；某條 Link 的發送端有空位時呼叫
//! [`Bridge::poll_transmit`]（帶 `now` 才能做逾期丟棄，指定 Link 才不會讓忙碌的 Link 擋住其他出口）；
//! 依 [`Bridge::poll_timeout`] 呼叫 [`Bridge::handle_timeout`]。
//!
//! 入口的檢查順序（`docs/30-safety-security/03` 第四節）：解析、查轉送表（含方向）、限流，最後放進出口佇列。
//! E2E 與 MAC 欄位原樣轉送；Transfer-ID、Tail byte、多框 CRC、datagram 標頭依出口重做。
//! 跨 Trust zone 的 MAC 驗證在 P6 加入。
#![no_std]

use embnet_core::Micros;
use embnet_core::can::{CanId, Mtu, NodeId, Priority, RxError, Segmenter, ServiceId, SessionTable, SubjectId, Tail};
use embnet_core::datagram::{self, Header};
use embnet_core::remap::TidRemap;

/// 一條 Link 的種類。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    /// CAN，帶 MTU。
    Can(Mtu),
    /// UART 或 UDP 的 datagram。
    Datagram,
}

impl LinkKind {
    /// Transfer-ID 的模數：CAN 32，datagram 256。
    pub const fn tid_modulo(self) -> u32 {
        match self {
            LinkKind::Can(_) => 32,
            LinkKind::Datagram => 256,
        }
    }
}

/// 轉送的對象。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Port {
    /// Subject：從入口送往出口。
    Subject(SubjectId),
    /// Service：request 從入口送往出口，response 反向回來。
    Service(ServiceId),
}

/// 出口佇列滿時的丟棄策略。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropPolicy {
    /// 丟最舊的（狀態類）。
    Oldest,
    /// 丟新來的（指令類）。
    Newest,
}

/// 一條轉送規則。
#[derive(Clone, Copy, Debug)]
pub struct Rule {
    /// 對象。
    pub port: Port,
    /// Subject 的發佈者，或 Service 的 client。
    pub source: NodeId,
    /// Service 的 server；Subject 時不用。
    pub server: Option<NodeId>,
    /// 入口 Link 的索引。
    pub ingress: u8,
    /// 出口 Link 的索引。
    pub egress: u8,
    /// 限流：平均每 `period_us` 一筆。
    pub period_us: u64,
    /// 限流：可以連續放行的筆數。
    pub burst: u32,
    /// 丟棄策略。
    pub drop: DropPolicy,
    /// 在出口佇列裡最多可以等多久；超過就在出隊時丟棄。
    pub deadline_us: Option<Micros>,
}

/// 計數。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// 已放進出口佇列的 Transfer。
    pub forwarded: u32,
    /// 轉送表沒有列出的應用流量；可能是設定錯誤或注入，計入 security 事件。
    pub no_route: u32,
    /// 沒有轉送的協定保留 Subject（8000 以上，例如 Heartbeat、E2E Sync）；它們本來就只在本地匯流排。
    pub local: u32,
    /// 被限流丟棄。
    pub rate_limited: u32,
    /// 出口佇列滿而丟棄的 Transfer。
    pub queue_dropped: u32,
    /// 逾期而丟棄的框。
    pub deadline_dropped: u32,
    /// CAN ID 或 datagram 格式錯誤。
    pub malformed: u32,
    /// 重組失敗。
    pub reassembly: u32,
    /// 太長，放不進緩衝。
    pub too_long: u32,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    key: u32,
    order: u32,
    rule: u8,
    len: u16,
    deadline: Option<Micros>,
    can_id: u32,
    data: [u8; 64],
    big: bool,
}

#[derive(Clone, Copy, Debug)]
struct RuleState {
    rule: Rule,
    remap: TidRemap,
    tokens: u64,
    last_refill: Micros,
    cut_drop: bool,
}

/// 一個邏輯上的 Transfer 標頭。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Logical {
    priority: Priority,
    port: Port,
    request: bool,
    source: NodeId,
    destination: Option<NodeId>,
    tid: u8,
}

/// `poll_transmit` 的輸出。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxOut {
    /// 一個 CAN 框。
    Can {
        /// 29 位元 ID。
        id: u32,
        /// 資料長度。
        len: usize,
    },
    /// 一個 datagram（UART 要再經 COBS）。
    Datagram {
        /// 長度。
        len: usize,
    },
}

/// Bridge 的固定容量。
///
/// - `L`：Link 數；`N`：轉送規則數；`QL`：每條出口的佇列格數（CAN 一框一格，datagram 一筆一格）。
/// - `R`：每條 CAN 入口的重組 session 數；`B`：重組與 datagram 的緩衝（≥ 型別最大長度 + 保護開銷 + 12）。
pub struct Bridge<const L: usize, const N: usize, const QL: usize, const R: usize, const B: usize> {
    links: [LinkKind; L],
    rules: [Option<RuleState>; N],
    queues: [[Option<Entry>; QL]; L],
    big: [[[u8; B]; QL]; L],
    sessions: [SessionTable<R, B>; L],
    order: u32,
    /// 計數。
    pub stats: Stats,
}

fn port_raw(p: Port) -> u16 {
    match p {
        Port::Subject(s) => s.get(),
        Port::Service(s) => s.get(),
    }
}

impl<const L: usize, const N: usize, const QL: usize, const R: usize, const B: usize> Bridge<L, N, QL, R, B> {
    /// 建立 Bridge。
    pub fn new(links: [LinkKind; L]) -> Self {
        Self {
            links,
            rules: [None; N],
            queues: [[None; QL]; L],
            big: [[[0; B]; QL]; L],
            sessions: [const { SessionTable::new() }; L],
            order: 0,
            stats: Stats::default(),
        }
    }

    /// 加入一條規則。容量已滿或 Link 索引超出時回傳 false。
    pub fn add_rule(&mut self, rule: Rule, now: Micros) -> bool {
        if usize::from(rule.ingress) >= L || usize::from(rule.egress) >= L || rule.ingress == rule.egress {
            return false;
        }
        let tokens = rule.period_us.saturating_mul(u64::from(rule.burst));
        match self.rules.iter_mut().find(|r| r.is_none()) {
            Some(slot) => {
                *slot = Some(RuleState { rule, remap: TidRemap::new(), tokens, last_refill: now, cut_drop: false });
                true
            }
            None => false,
        }
    }

    /// 某條出口佇列目前的格數。
    pub fn queued(&self, link: usize) -> usize {
        self.queues.get(link).map_or(0, |q| q.iter().filter(|e| e.is_some()).count())
    }

    /// 找出符合的規則：Subject 與 request 依入口找，response 依出口反向找。回傳（規則索引、是否反向）。
    fn matches(&self, link: u8, l: &Logical) -> impl Iterator<Item = (usize, bool)> + '_ {
        let l = *l;
        self.rules.iter().enumerate().filter_map(move |(i, r)| {
            let r = r.as_ref()?.rule;
            match (r.port, l.port) {
                (Port::Subject(a), Port::Subject(b)) if a == b && r.source == l.source && r.ingress == link => Some((i, false)),
                (Port::Service(a), Port::Service(b)) if a == b => {
                    if l.request && r.ingress == link && r.source == l.source && r.server == l.destination {
                        Some((i, false))
                    } else if !l.request && r.egress == link && r.server == Some(l.source) && l.destination == Some(r.source) {
                        Some((i, true))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        })
    }

    fn count_unrouted(&mut self, l: &Logical) {
        match l.port {
            Port::Subject(s) if s.get() >= 8000 => self.stats.local = self.stats.local.saturating_add(1),
            _ => self.stats.no_route = self.stats.no_route.saturating_add(1),
        }
    }

    /// 限流：token bucket，單位是微秒；每筆消耗一個週期。
    fn admit(&mut self, i: usize, now: Micros) -> bool {
        let Some(Some(st)) = self.rules.get_mut(i) else { return false };
        let cap = st.rule.period_us.saturating_mul(u64::from(st.rule.burst.max(1)));
        st.tokens = st.tokens.saturating_add(now.saturating_sub(st.last_refill)).min(cap);
        st.last_refill = now;
        if st.tokens >= st.rule.period_us {
            st.tokens = st.tokens.saturating_sub(st.rule.period_us);
            true
        } else {
            self.stats.rate_limited = self.stats.rate_limited.saturating_add(1);
            false
        }
    }

    /// 依規則算出口的 Transfer-ID（`docs/spec/06-forwarding.md` 第三節）。
    fn out_tid(&mut self, i: usize, reverse: bool, in_tid: u8) -> Option<u8> {
        let st = self.rules.get_mut(i)?.as_mut()?;
        let (m_in, m_out) = {
            let a = self.links.get(usize::from(st.rule.ingress))?.tid_modulo();
            let b = self.links.get(usize::from(st.rule.egress))?.tid_modulo();
            if reverse { (b, a) } else { (a, b) }
        };
        if !reverse {
            st.remap.ingress(in_tid, m_in);
            return st.remap.egress(m_out);
        }
        // response：在 request 的展開值之下，找最近一個低位等於 in_tid 的值。
        // request 送出時用的是 U mod m_out，server 原樣回送，所以 U - ((U - t) mod m_in) 就是原 request 的展開值。
        let u = st.remap.value()?;
        let mask = m_in.wrapping_sub(1);
        let back = u.wrapping_sub(u.wrapping_sub(u32::from(in_tid)) & mask);
        Some((back & m_out.wrapping_sub(1)) as u8)
    }

    fn free(&self, link: usize) -> usize {
        self.queues.get(link).map_or(0, |q| q.iter().filter(|e| e.is_none()).count())
    }

    /// 丟最舊的、屬於同一條規則的格，直到有 `need` 格空位；做不到就回傳 false。
    fn make_room(&mut self, link: usize, rule: u8, need: usize) -> bool {
        while self.free(link) < need {
            let Some(q) = self.queues.get_mut(link) else { return false };
            let oldest = q
                .iter()
                .enumerate()
                .filter_map(|(i, e)| e.filter(|e| e.rule == rule).map(|e| (i, e.order)))
                .min_by_key(|&(_, o)| o.wrapping_sub(self.order))
                .map(|(i, _)| i);
            match oldest.and_then(|i| q.get_mut(i)) {
                Some(slot) => *slot = None,
                None => return false,
            }
        }
        true
    }

    fn push(&mut self, link: usize, e: Entry, big: Option<&[u8]>) -> bool {
        let Some(q) = self.queues.get_mut(link) else { return false };
        let Some(idx) = q.iter().position(Option::is_none) else { return false };
        if let Some(b) = big
            && let Some(dst) = self.big.get_mut(link).and_then(|x| x.get_mut(idx)).and_then(|x| x.get_mut(..b.len()))
        {
            dst.copy_from_slice(b);
        }
        if let Some(slot) = q.get_mut(idx) {
            *slot = Some(e);
        }
        self.order = self.order.wrapping_add(1);
        true
    }

    /// 把一筆完整的 Transfer 放進規則的出口。
    fn enqueue_transfer(&mut self, now: Micros, i: usize, reverse: bool, l: &Logical, payload: &[u8]) {
        let Some(Some(st)) = self.rules.get(i).copied() else { return };
        let r = st.rule;
        let out_link = if reverse { r.ingress } else { r.egress };
        let link = usize::from(out_link);
        let Some(kind) = self.links.get(link).copied() else { return };
        let Some(tid) = self.out_tid(i, reverse, l.tid) else { return };
        let deadline = r.deadline_us.map(|d| now.saturating_add(d));
        let rule = i as u8;
        match kind {
            LinkKind::Can(mtu) => {
                let mut seg = Segmenter::new(payload, mtu, tid);
                let need = seg.frame_count();
                if !self.room(link, rule, need, r.drop) {
                    return;
                }
                let id = match l.port {
                    Port::Subject(subject) => CanId::Subject { priority: l.priority, subject, source: l.source },
                    Port::Service(service) => CanId::Service {
                        priority: l.priority,
                        request: l.request,
                        service,
                        destination: l.destination.unwrap_or(l.source),
                        source: l.source,
                    },
                }
                .encode();
                let mut buf = [0u8; 64];
                while let Some(n) = seg.next_frame(&mut buf) {
                    let e = Entry { key: id, order: self.order, rule, len: n as u16, deadline, can_id: id, data: buf, big: false };
                    self.push(link, e, None);
                }
            }
            LinkKind::Datagram => {
                let h = Header {
                    priority: l.priority.get(),
                    service: matches!(l.port, Port::Service(_)),
                    request: l.request,
                    port: port_raw(l.port),
                    source: l.source.get(),
                    destination: l.destination.map_or(datagram::NO_DESTINATION, NodeId::get),
                    transfer_id: tid,
                };
                let mut buf = [0u8; B];
                let Ok(n) = datagram::encode(&h, payload, &mut buf) else {
                    self.stats.too_long = self.stats.too_long.saturating_add(1);
                    return;
                };
                if !self.room(link, rule, 1, r.drop) {
                    return;
                }
                let key = u32::from(l.priority.get()) << 26;
                let e = Entry { key, order: self.order, rule, len: n as u16, deadline, can_id: 0, data: [0; 64], big: true };
                self.push(link, e, buf.get(..n));
            }
        }
        self.stats.forwarded = self.stats.forwarded.saturating_add(1);
    }

    fn room(&mut self, link: usize, rule: u8, need: usize, drop: DropPolicy) -> bool {
        if self.free(link) >= need {
            return true;
        }
        let ok = drop == DropPolicy::Oldest && need <= QL && self.make_room(link, rule, need);
        if !ok {
            self.stats.queue_dropped = self.stats.queue_dropped.saturating_add(1);
        }
        ok
    }

    /// 處理一個從 CAN Link 收到的框。
    pub fn on_can(&mut self, link: u8, now: Micros, raw_id: u32, data: &[u8]) {
        let Some(LinkKind::Can(in_mtu)) = self.links.get(usize::from(link)).copied() else { return };
        let Ok(id) = CanId::decode(raw_id) else {
            self.stats.malformed = self.stats.malformed.saturating_add(1);
            return;
        };
        let Some(&tail_byte) = data.last() else {
            self.stats.malformed = self.stats.malformed.saturating_add(1);
            return;
        };
        let tail = Tail::decode(tail_byte);
        let l = match id {
            CanId::Subject { priority, subject, source } => {
                Logical { priority, port: Port::Subject(subject), request: false, source, destination: None, tid: tail.transfer_id }
            }
            CanId::Service { priority, request, service, destination, source } => Logical {
                priority,
                port: Port::Service(service),
                request,
                source,
                destination: Some(destination),
                tid: tail.transfer_id,
            },
        };
        let mut found = [(0usize, false); 8];
        let mut n = 0usize;
        for m in self.matches(link, &l) {
            if let Some(slot) = found.get_mut(n) {
                *slot = m;
                n = n.saturating_add(1);
            }
        }
        if n == 0 {
            if tail.sot {
                self.count_unrouted(&l);
            }
            return;
        }
        // 每條規則決定逐框直通或先重組
        let mut need_reassembly = false;
        for &(i, reverse) in found.get(..n).unwrap_or(&[]) {
            let Some(Some(st)) = self.rules.get(i).copied() else { continue };
            let out = if reverse { st.rule.ingress } else { st.rule.egress };
            if self.links.get(usize::from(out)).copied() == Some(LinkKind::Can(in_mtu)) {
                self.cut_through(now, i, reverse, id, &l, tail, data);
            } else {
                need_reassembly = true;
            }
        }
        if !need_reassembly {
            return;
        }
        let mut tmp = [0u8; B];
        let len = {
            let Some(table) = self.sessions.get_mut(usize::from(link)) else { return };
            match table.push(now, 1_000_000, id, data) {
                Ok(Some(a)) => {
                    let len = a.payload.len();
                    let Some(dst) = tmp.get_mut(..len) else { return };
                    dst.copy_from_slice(a.payload);
                    len
                }
                Ok(None) => return,
                Err(RxError::Duplicate) => return,
                Err(_) => {
                    self.stats.reassembly = self.stats.reassembly.saturating_add(1);
                    return;
                }
            }
        };
        let payload = tmp.get(..len).unwrap_or(&[]);
        for &(i, reverse) in found.get(..n).unwrap_or(&[]) {
            let Some(Some(st)) = self.rules.get(i).copied() else { continue };
            let out = if reverse { st.rule.ingress } else { st.rule.egress };
            if self.links.get(usize::from(out)).copied() == Some(LinkKind::Can(in_mtu)) {
                continue;
            }
            if self.admit(i, now) {
                self.enqueue_transfer(now, i, reverse, &l, payload);
            }
        }
    }

    /// 格式相同的 CAN 到 CAN：逐框轉送，Transfer-ID 不變（模數相同）。限流在 SOT 框決定，整筆一起放行或丟棄。
    #[allow(clippy::too_many_arguments)]
    fn cut_through(&mut self, now: Micros, i: usize, reverse: bool, id: CanId, l: &Logical, tail: Tail, data: &[u8]) {
        if tail.sot {
            let ok = self.admit(i, now);
            if let Some(Some(st)) = self.rules.get_mut(i) {
                st.cut_drop = !ok;
            }
            if ok {
                self.stats.forwarded = self.stats.forwarded.saturating_add(1);
                let _ = self.out_tid(i, reverse, l.tid);
            }
        }
        let Some(Some(st)) = self.rules.get(i).copied() else { return };
        if st.cut_drop {
            return;
        }
        let link = usize::from(if reverse { st.rule.ingress } else { st.rule.egress });
        let raw = id.encode();
        let mut buf = [0u8; 64];
        let Some(dst) = buf.get_mut(..data.len()) else { return };
        dst.copy_from_slice(data);
        if !self.room(link, i as u8, 1, st.rule.drop) {
            return;
        }
        let deadline = st.rule.deadline_us.map(|d| now.saturating_add(d));
        let e = Entry { key: raw, order: self.order, rule: i as u8, len: data.len() as u16, deadline, can_id: raw, data: buf, big: false };
        self.push(link, e, None);
    }

    /// 處理一個從 datagram Link（UART 解完 COBS 後、或 UDP）收到的 datagram。
    pub fn on_datagram(&mut self, link: u8, now: Micros, d: &[u8]) {
        if self.links.get(usize::from(link)).copied() != Some(LinkKind::Datagram) {
            return;
        }
        let Ok((h, payload)) = datagram::decode(d) else {
            self.stats.malformed = self.stats.malformed.saturating_add(1);
            return;
        };
        let (Some(priority), Some(source)) = (Priority::new(h.priority), NodeId::new(h.source)) else {
            self.stats.malformed = self.stats.malformed.saturating_add(1);
            return;
        };
        let port = if h.service {
            match ServiceId::new(h.port) {
                Some(s) => Port::Service(s),
                None => {
                    self.stats.malformed = self.stats.malformed.saturating_add(1);
                    return;
                }
            }
        } else {
            match SubjectId::new(h.port) {
                Some(s) => Port::Subject(s),
                None => {
                    self.stats.malformed = self.stats.malformed.saturating_add(1);
                    return;
                }
            }
        };
        let destination = if h.service { NodeId::new(h.destination) } else { None };
        let l = Logical { priority, port, request: h.request, source, destination, tid: h.transfer_id };
        let mut found = [(0usize, false); 8];
        let mut n = 0usize;
        for m in self.matches(link, &l) {
            if let Some(slot) = found.get_mut(n) {
                *slot = m;
                n = n.saturating_add(1);
            }
        }
        if n == 0 {
            self.count_unrouted(&l);
            return;
        }
        let mut tmp = [0u8; B];
        let Some(dst) = tmp.get_mut(..payload.len()) else {
            self.stats.too_long = self.stats.too_long.saturating_add(1);
            return;
        };
        dst.copy_from_slice(payload);
        let len = payload.len();
        for &(i, reverse) in found.get(..n).unwrap_or(&[]) {
            if self.admit(i, now) {
                self.enqueue_transfer(now, i, reverse, &l, tmp.get(..len).unwrap_or(&[]));
            }
        }
    }

    /// 取出某條 Link 下一個要送的框或 datagram，寫進 `out`。逾期的格在這裡丟棄。
    pub fn poll_transmit(&mut self, link: usize, now: Micros, out: &mut [u8]) -> Option<TxOut> {
        loop {
            let order = self.order;
            let q = self.queues.get_mut(link)?;
            let idx = q
                .iter()
                .enumerate()
                .filter_map(|(i, e)| e.map(|e| (i, e.key, e.order.wrapping_sub(order))))
                .min_by_key(|&(_, k, age)| (k, age))
                .map(|(i, _, _)| i)?;
            let e = q.get_mut(idx)?.take()?;
            if e.deadline.is_some_and(|d| now > d) {
                self.stats.deadline_dropped = self.stats.deadline_dropped.saturating_add(1);
                continue;
            }
            let n = usize::from(e.len);
            return if e.big {
                let src = self.big.get(link)?.get(idx)?.get(..n)?;
                out.get_mut(..n)?.copy_from_slice(src);
                Some(TxOut::Datagram { len: n })
            } else {
                out.get_mut(..n)?.copy_from_slice(e.data.get(..n)?);
                Some(TxOut::Can { id: e.can_id, len: n })
            };
        }
    }

    /// 清除逾時的重組狀態。
    pub fn handle_timeout(&mut self, now: Micros) {
        for t in self.sessions.iter_mut() {
            t.handle_timeout(now, 1_000_000);
        }
    }
}
