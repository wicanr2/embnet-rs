//! embnet 的 Native node，CAN Link 版本。
//!
//! sans-IO：Node 不碰 I/O 也不讀時鐘（`docs/40-rust-embedded/02`）。外層負責：
//!
//! - 收到一個 CAN 框就呼叫 [`Node::on_frame`]，拿回可能的 [`Event`]。
//! - 發送端有空位時反覆呼叫 [`Node::poll_transmit`]，把框送上線。
//! - 依 [`Node::poll_timeout`] 回報的時刻呼叫 [`Node::handle_timeout`]。
//!
//! 線格式依 `docs/spec/01` 到 `03`；E2E 依 `docs/spec/02-e2e.md`。所有容量都是 const generic，不配置記憶體。
#![no_std]

use embnet_core::Micros;
use embnet_core::can::{CanId, Mtu, NodeId, Priority, RxError, Segmenter, ServiceId, SessionTable, SubjectId};
use embnet_core::e2e::{self, DataId, SeqReceiver, StateMachine, StateParams, Status, StreamState};
use embnet_core::heartbeat::{self, Heartbeat};

// 協定保留的常數。值都在範圍內，在編譯期求值；不在範圍內時無法編譯。
#[allow(clippy::panic)]
const PRIO_4: Priority = match Priority::new(4) {
    Some(p) => p,
    None => panic!(),
};
#[allow(clippy::panic)]
const HB_SUBJECT: SubjectId = match SubjectId::new(heartbeat::SUBJECT_ID) {
    Some(s) => s,
    None => panic!(),
};
#[allow(clippy::panic)]
const SYNC_SUBJECT: SubjectId = match SubjectId::new(e2e::SYNC_SUBJECT_ID) {
    Some(s) => s,
    None => panic!(),
};

/// 一個待送的 CAN 框。
#[derive(Clone, Copy, Debug)]
struct TxFrame {
    id: u32,
    order: u32,
    len: u8,
    data: [u8; 64],
}

/// 發佈端的保護方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PubProtection {
    /// 不保護。
    None,
    /// E2E-32，帶型別雜湊。
    E2e32 {
        /// 型別雜湊。
        type_hash: u64,
    },
}

/// 一個發佈的設定。
#[derive(Clone, Copy, Debug)]
pub struct Publication {
    /// Subject-ID。
    pub subject: SubjectId,
    /// 優先等級。
    pub priority: Priority,
    /// 保護方式。
    pub protection: PubProtection,
}

/// 訂閱端的保護方式。
#[derive(Clone, Copy, Debug)]
pub enum SubProtection {
    /// 不保護。
    None,
    /// E2E-32。
    E2e32 {
        /// 型別雜湊。
        type_hash: u64,
        /// 容許的跳號。
        max_delta: u8,
        /// 期限，微秒；超過沒收到就產生一次 NONEWDATA。
        deadline: Micros,
        /// 狀態機參數。
        params: StateParams,
    },
}

/// 一個訂閱的設定。
#[derive(Clone, Copy, Debug)]
pub struct Subscription {
    /// Subject-ID。
    pub subject: SubjectId,
    /// 保護方式。
    pub protection: SubProtection,
    /// Transfer-ID 逾時，微秒。
    pub tid_timeout: Micros,
}

/// Node 回報給應用的事件。payload 借自 Node 內部的緩衝，下一次呼叫 `on_frame` 前有效。
#[derive(Debug, PartialEq, Eq)]
pub enum Event<'a> {
    /// 收到一筆訂閱的 Subject。
    Message {
        /// Subject-ID。
        subject: SubjectId,
        /// 來源。
        source: NodeId,
        /// 應用資料；`protected` 時已去掉 E2E 欄位。
        payload: &'a [u8],
        /// `protected` 時的 E2E 結果與資料流狀態。
        e2e: Option<(Status, StreamState)>,
    },
    /// `protected` Subject 的一筆結果不能交給應用。
    Rejected {
        /// Subject-ID。
        subject: SubjectId,
        /// 來源。
        source: NodeId,
        /// 結果。
        status: Status,
        /// 資料流狀態。
        state: StreamState,
    },
    /// 收到別的 Node 的 Heartbeat。
    Heartbeat {
        /// 來源。
        source: NodeId,
        /// 內容。
        heartbeat: Heartbeat,
    },
    /// 收到給自己的 Service request。
    Request {
        /// Service-ID。
        service: ServiceId,
        /// client。
        source: NodeId,
        /// 回應時要沿用的 Transfer-ID。
        transfer_id: u8,
        /// payload。
        payload: &'a [u8],
    },
    /// 收到自己發出的 request 的 response。
    Response {
        /// Service-ID。
        service: ServiceId,
        /// server。
        source: NodeId,
        /// Transfer-ID。
        transfer_id: u8,
        /// payload。
        payload: &'a [u8],
    },
}

/// API 呼叫失敗的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// 沒有這個發佈的設定。
    NotPublished,
    /// 發送佇列放不下整筆 Transfer。
    TxFull,
    /// payload 太長。
    TooLong,
    /// 進行中的 request 已達上限。
    PendingFull,
}

/// 計數（`docs/spec/01-can-transport.md` 第七節）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// CAN ID 的保留位元錯誤。
    pub reserved: u32,
    /// 長度為 0 的框。
    pub empty: u32,
    /// toggle 錯誤。
    pub toggle: u32,
    /// 沒有對應狀態的中間框。
    pub no_session: u32,
    /// 累積超過緩衝。
    pub overflow: u32,
    /// 多框 CRC 錯誤。
    pub crc: u32,
    /// 重複。
    pub duplicate: u32,
    /// 重組狀態用完。
    pub exhausted: u32,
    /// 逾時清除的重組狀態。
    pub reassembly_timeout: u32,
    /// E2E 的各種結果，依 [`Status`] 代碼為索引。
    pub e2e: [u32; 7],
    /// 資料流數量已達上限，新的來源被忽略。
    pub streams_full: u32,
    /// 發送佇列滿。
    pub tx_full: u32,
    /// 逾時的 request。
    pub request_timeout: u32,
    /// 沒人訂閱或不是給自己的框。
    pub ignored: u32,
}

impl Stats {
    fn count_rx(&mut self, e: RxError) {
        let c = match e {
            RxError::Empty => &mut self.empty,
            RxError::BadToggle => &mut self.toggle,
            RxError::NoSession => &mut self.no_session,
            RxError::Overflow => &mut self.overflow,
            RxError::Crc => &mut self.crc,
            RxError::Duplicate => &mut self.duplicate,
            RxError::Exhausted => &mut self.exhausted,
        };
        *c = c.saturating_add(1);
    }

    fn count_e2e(&mut self, s: Status) {
        if let Some(c) = self.e2e.get_mut(usize::from(s as u8)) {
            *c = c.saturating_add(1);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PubState {
    cfg: Publication,
    tid: u8,
    seq: u32,
}

#[derive(Clone, Copy, Debug)]
struct Stream {
    subject: SubjectId,
    source: NodeId,
    seq: SeqReceiver,
    sm: StateMachine,
    last_rx: Micros,
}

#[derive(Clone, Copy, Debug)]
struct Pending {
    service: ServiceId,
    server: NodeId,
    tid: u8,
    deadline: Micros,
}

/// Node 的固定容量。
///
/// - `P`：發佈數；`S`：訂閱數；`D`：`protected` 的資料流數（Subject × 來源）。
/// - `R`：重組 session 數；`B`：每個 session 的緩衝（≥ 型別最大長度 + E2E 開銷 + 2）。
/// - `Q`：發送佇列的框數；`K`：同時進行中的 request 數。
pub struct Node<const P: usize, const S: usize, const D: usize, const R: usize, const B: usize, const Q: usize, const K: usize> {
    id: NodeId,
    mtu: Mtu,
    boot: Micros,
    pubs: [Option<PubState>; P],
    subs: [Option<Subscription>; S],
    streams: [Option<Stream>; D],
    sessions: SessionTable<R, B>,
    txq: [Option<TxFrame>; Q],
    tx_order: u32,
    pending: [Option<Pending>; K],
    svc_tid: [(u16, u8, u8); K],
    hb_next: Micros,
    hb_tid: u8,
    sync_tid: u8,
    /// Heartbeat 的 health、mode、vendor 等欄位，由應用設定。
    pub status: Heartbeat,
    /// 計數。
    pub stats: Stats,
    e2e_buf: [u8; B],
}

impl<const P: usize, const S: usize, const D: usize, const R: usize, const B: usize, const Q: usize, const K: usize>
    Node<P, S, D, R, B, Q, K>
{
    /// 建立 Node。開機後的第一個 Heartbeat 與 E2E Sync 在第一次 `handle_timeout` 時送出。
    pub fn new(id: NodeId, mtu: Mtu, now: Micros) -> Self {
        Self {
            id,
            mtu,
            boot: now,
            pubs: [None; P],
            subs: [None; S],
            streams: [None; D],
            sessions: SessionTable::new(),
            txq: [None; Q],
            tx_order: 0,
            pending: [None; K],
            svc_tid: [(u16::MAX, 0, 0); K],
            hb_next: now,
            hb_tid: 0,
            sync_tid: 0,
            status: Heartbeat {
                uptime: 0,
                health: 0,
                mode: 0,
                fault_log_unread: false,
                auth_disabled: true,
                can_state: 0,
                vendor: 0,
            },
            stats: Stats::default(),
            e2e_buf: [0; B],
        }
    }

    /// 這個 Node 的 Node-ID。
    pub const fn id(&self) -> NodeId {
        self.id
    }

    /// 加入一個發佈。容量已滿時回傳 false。
    pub fn add_publication(&mut self, cfg: Publication) -> bool {
        match self.pubs.iter_mut().find(|p| p.is_none()) {
            Some(slot) => {
                *slot = Some(PubState { cfg, tid: 0, seq: 0 });
                true
            }
            None => false,
        }
    }

    /// 加入一個訂閱。容量已滿時回傳 false。
    pub fn add_subscription(&mut self, cfg: Subscription) -> bool {
        match self.subs.iter_mut().find(|s| s.is_none()) {
            Some(slot) => {
                *slot = Some(cfg);
                true
            }
            None => false,
        }
    }

    fn free_tx_slots(&self) -> usize {
        self.txq.iter().filter(|f| f.is_none()).count()
    }

    fn enqueue(&mut self, id: CanId, tid: u8, payload: &[u8]) -> Result<(), Error> {
        let mut seg = Segmenter::new(payload, self.mtu, tid);
        if seg.frame_count() > self.free_tx_slots() {
            self.stats.tx_full = self.stats.tx_full.saturating_add(1);
            return Err(Error::TxFull);
        }
        let raw = id.encode();
        let mut buf = [0u8; 64];
        while let Some(n) = seg.next_frame(&mut buf) {
            let slot = self.txq.iter_mut().find(|f| f.is_none()).ok_or(Error::TxFull)?;
            *slot = Some(TxFrame { id: raw, order: self.tx_order, len: n as u8, data: buf });
            self.tx_order = self.tx_order.wrapping_add(1);
        }
        Ok(())
    }

    /// 發佈一筆。`protected` 時自動加上 E2E 欄位。
    pub fn publish(&mut self, subject: SubjectId, payload: &[u8]) -> Result<(), Error> {
        let src = self.id;
        let idx = self.pubs.iter().position(|p| p.is_some_and(|p| p.cfg.subject == subject)).ok_or(Error::NotPublished)?;
        let st = self.pubs.get(idx).copied().flatten().ok_or(Error::NotPublished)?;
        let id = CanId::Subject { priority: st.cfg.priority, subject, source: src };
        match st.cfg.protection {
            PubProtection::None => self.enqueue(id, st.tid, payload)?,
            PubProtection::E2e32 { type_hash } => {
                let did = DataId { subject_id: subject.get(), source: src.get(), type_hash };
                let mut buf = [0u8; B];
                let n = e2e::encode(&did, st.seq, payload, &mut buf).ok_or(Error::TooLong)?;
                self.enqueue(id, st.tid, buf.get(..n).ok_or(Error::TooLong)?)?;
            }
        }
        if let Some(Some(p)) = self.pubs.get_mut(idx) {
            p.tid = p.tid.wrapping_add(1) & 0x1F;
            if matches!(p.cfg.protection, PubProtection::E2e32 { .. }) {
                p.seq = p.seq.wrapping_add(1);
            }
        }
        Ok(())
    }

    /// 送出一筆 Service request，回傳使用的 Transfer-ID。
    pub fn request(&mut self, now: Micros, service: ServiceId, server: NodeId, priority: Priority, timeout: Micros, payload: &[u8]) -> Result<u8, Error> {
        let slot_idx = self.pending.iter().position(Option::is_none).ok_or(Error::PendingFull)?;
        let key = (service.get(), server.get());
        let ti = match self.svc_tid.iter().position(|e| (e.0, e.1) == key) {
            Some(i) => i,
            None => {
                let i = self.svc_tid.iter().position(|e| e.0 == u16::MAX).unwrap_or(0);
                if let Some(e) = self.svc_tid.get_mut(i) {
                    *e = (key.0, key.1, 0);
                }
                i
            }
        };
        let tid = self.svc_tid.get(ti).map_or(0, |e| e.2);
        let id = CanId::Service { priority, request: true, service, destination: server, source: self.id };
        self.enqueue(id, tid, payload)?;
        if let Some(e) = self.svc_tid.get_mut(ti) {
            e.2 = tid.wrapping_add(1) & 0x1F;
        }
        if let Some(slot) = self.pending.get_mut(slot_idx) {
            *slot = Some(Pending { service, server, tid, deadline: now.saturating_add(timeout) });
        }
        Ok(tid)
    }

    /// 回應一筆 request。
    pub fn respond(&mut self, service: ServiceId, client: NodeId, transfer_id: u8, priority: Priority, payload: &[u8]) -> Result<(), Error> {
        let id = CanId::Service { priority, request: false, service, destination: client, source: self.id };
        self.enqueue(id, transfer_id, payload)
    }

    /// 取出下一個要送的框：CAN ID 最小者優先，同 ID 依進入佇列的順序（同一 session 不亂序，`003` K-29）。
    pub fn poll_transmit(&mut self, out: &mut [u8]) -> Option<(u32, usize)> {
        let idx = self
            .txq
            .iter()
            .enumerate()
            .filter_map(|(i, f)| f.map(|f| (i, f.id, f.order.wrapping_sub(self.tx_order))))
            .min_by_key(|&(_, id, age)| (id, age))
            .map(|(i, _, _)| i)?;
        let f = self.txq.get_mut(idx)?.take()?;
        let n = usize::from(f.len);
        out.get_mut(..n)?.copy_from_slice(f.data.get(..n)?);
        Some((f.id, n))
    }

    /// 發送佇列中還有幾框。
    pub fn tx_pending(&self) -> usize {
        Q.saturating_sub(self.free_tx_slots())
    }

    fn stream_index(&mut self, subject: SubjectId, source: NodeId, max_delta: u8, params: StateParams, now: Micros) -> Option<usize> {
        if let Some(i) = self.streams.iter().position(|s| s.is_some_and(|s| s.subject == subject && s.source == source)) {
            return Some(i);
        }
        let Some(i) = self.streams.iter().position(Option::is_none) else {
            self.stats.streams_full = self.stats.streams_full.saturating_add(1);
            return None;
        };
        if let Some(slot) = self.streams.get_mut(i) {
            *slot = Some(Stream { subject, source, seq: SeqReceiver::new(max_delta), sm: StateMachine::new(params), last_rx: now });
        }
        Some(i)
    }

    fn handle_sync(&mut self, source: NodeId, payload: &[u8], now: Micros) {
        let Some(entries) = e2e::sync_decode(source.get(), payload) else {
            self.stats.count_e2e(Status::Error);
            return;
        };
        let mut list = [(0u16, 0u32); 16];
        let mut n = 0usize;
        for e in entries {
            if let Some(slot) = list.get_mut(n) {
                *slot = e;
                n = n.saturating_add(1);
            }
        }
        for &(sid, next) in list.get(..n).unwrap_or(&[]) {
            let Some(subject) = SubjectId::new(sid) else { continue };
            let Some(sub) = self.subs.iter().flatten().find(|s| s.subject == subject).copied() else { continue };
            let SubProtection::E2e32 { max_delta, params, .. } = sub.protection else { continue };
            if let Some(i) = self.stream_index(subject, source, max_delta, params, now)
                && let Some(Some(st)) = self.streams.get_mut(i)
                && (!st.seq.synced() || st.sm.state() == StreamState::Invalid)
            {
                st.seq.sync(next);
            }
        }
    }

    /// 處理一個收到的 CAN 框。
    pub fn on_frame(&mut self, now: Micros, raw_id: u32, data: &[u8]) -> Option<Event<'_>> {
        let id = match CanId::decode(raw_id) {
            Ok(id) => id,
            Err(_) => {
                self.stats.reserved = self.stats.reserved.saturating_add(1);
                return None;
            }
        };
        match id {
            CanId::Subject { subject, source, .. } => {
                let is_hb = subject.get() == heartbeat::SUBJECT_ID;
                let is_sync = subject.get() == e2e::SYNC_SUBJECT_ID;
                let sub = self.subs.iter().flatten().find(|s| s.subject == subject).copied();
                if !is_hb && !is_sync && sub.is_none() {
                    self.stats.ignored = self.stats.ignored.saturating_add(1);
                    return None;
                }
                let timeout = sub.map_or(1_000_000, |s| s.tid_timeout);
                // 先把 payload 複製出來，避免同時借用 session 表與 self
                let mut tmp = [0u8; B];
                let n = match self.sessions.push(now, timeout, id, data) {
                    Ok(Some(a)) => {
                        let n = a.payload.len();
                        tmp.get_mut(..n)?.copy_from_slice(a.payload);
                        n
                    }
                    Ok(None) => return None,
                    Err(e) => {
                        self.stats.count_rx(e);
                        return None;
                    }
                };
                let payload = tmp.get(..n)?;
                if is_hb {
                    return Heartbeat::decode(payload).map(|heartbeat| Event::Heartbeat { source, heartbeat });
                }
                if is_sync {
                    self.handle_sync(source, payload, now);
                    return None;
                }
                let sub = sub?;
                match sub.protection {
                    SubProtection::None => {
                        self.e2e_buf.get_mut(..n)?.copy_from_slice(payload);
                        Some(Event::Message { subject, source, payload: self.e2e_buf.get(..n)?, e2e: None })
                    }
                    SubProtection::E2e32 { type_hash, max_delta, params, .. } => {
                        let i = self.stream_index(subject, source, max_delta, params, now)?;
                        let did = DataId { subject_id: subject.get(), source: source.get(), type_hash };
                        let st = self.streams.get_mut(i)?.as_mut()?;
                        let (status, data) = st.seq.check(&did, payload);
                        let state = st.sm.push(status);
                        st.last_rx = now;
                        self.stats.count_e2e(status);
                        match data {
                            Some(d) => {
                                let m = d.len();
                                self.e2e_buf.get_mut(..m)?.copy_from_slice(d);
                                Some(Event::Message { subject, source, payload: self.e2e_buf.get(..m)?, e2e: Some((status, state)) })
                            }
                            None => Some(Event::Rejected { subject, source, status, state }),
                        }
                    }
                }
            }
            CanId::Service { request, service, destination, source, .. } => {
                if destination != self.id {
                    self.stats.ignored = self.stats.ignored.saturating_add(1);
                    return None;
                }
                let (tid, n) = match self.sessions.push(now, 1_000_000, id, data) {
                    Ok(Some(a)) => {
                        let n = a.payload.len();
                        self.e2e_buf.get_mut(..n)?.copy_from_slice(a.payload);
                        (a.transfer_id, n)
                    }
                    Ok(None) => return None,
                    Err(e) => {
                        self.stats.count_rx(e);
                        return None;
                    }
                };
                if request {
                    return Some(Event::Request { service, source, transfer_id: tid, payload: self.e2e_buf.get(..n)? });
                }
                let p = self.pending.iter().position(|p| {
                    p.is_some_and(|p| p.service == service && p.server == source && p.tid == tid && now <= p.deadline)
                })?;
                if let Some(slot) = self.pending.get_mut(p) {
                    *slot = None;
                }
                Some(Event::Response { service, source, transfer_id: tid, payload: self.e2e_buf.get(..n)? })
            }
        }
    }

    /// 處理到期的工作：Heartbeat 與 E2E Sync、重組逾時、E2E 期限、request 逾時。
    pub fn handle_timeout(&mut self, now: Micros) {
        let cleared = self.sessions.handle_timeout(now, 1_000_000);
        self.stats.reassembly_timeout = self.stats.reassembly_timeout.saturating_add(cleared as u32);

        for p in self.pending.iter_mut() {
            if p.is_some_and(|x| now > x.deadline) {
                *p = None;
                self.stats.request_timeout = self.stats.request_timeout.saturating_add(1);
            }
        }

        let subs = self.subs;
        for st in self.streams.iter_mut().flatten() {
            let deadline = subs.iter().flatten().find(|s| s.subject == st.subject).and_then(|s| match s.protection {
                SubProtection::E2e32 { deadline, .. } => Some(deadline),
                SubProtection::None => None,
            });
            if let Some(d) = deadline
                && now.saturating_sub(st.last_rx) > d
            {
                st.sm.push(Status::NoNewData);
                st.last_rx = now;
                if let Some(c) = self.stats.e2e.get_mut(usize::from(Status::NoNewData as u8)) {
                    *c = c.saturating_add(1);
                }
            }
        }

        if now >= self.hb_next {
            self.hb_next = now.saturating_add(heartbeat::PERIOD_US);
            let mut hb = self.status;
            hb.uptime = u32::try_from(now.saturating_sub(self.boot) / 1_000_000).unwrap_or(u32::MAX);
            let payload = hb.encode();
            let id = CanId::Subject {
                priority: PRIO_4,
                subject: HB_SUBJECT,
                source: self.id,
            };
            let tid = self.hb_tid;
            if self.enqueue(id, tid, &payload).is_ok() {
                self.hb_tid = tid.wrapping_add(1) & 0x1F;
            }
            self.send_sync();
        }
    }

    fn send_sync(&mut self) {
        let mut entries = [(0u16, 0u32); 16];
        let mut n = 0usize;
        for p in self.pubs.iter().flatten() {
            if matches!(p.cfg.protection, PubProtection::E2e32 { .. })
                && let Some(slot) = entries.get_mut(n)
            {
                *slot = (p.cfg.subject.get(), p.seq);
                n = n.saturating_add(1);
            }
        }
        if n == 0 {
            return;
        }
        let mut buf = [0u8; 128];
        let Some(len) = e2e::sync_encode(self.id.get(), entries.get(..n).unwrap_or(&[]), &mut buf) else { return };
        let id = CanId::Subject {
            priority: PRIO_4,
            subject: SYNC_SUBJECT,
            source: self.id,
        };
        let tid = self.sync_tid;
        if self.enqueue(id, tid, buf.get(..len).unwrap_or(&[])).is_ok() {
            self.sync_tid = tid.wrapping_add(1) & 0x1F;
        }
    }

    /// 下一次需要呼叫 `handle_timeout` 的時刻。
    pub fn poll_timeout(&self) -> Micros {
        let mut t = self.hb_next;
        for p in self.pending.iter().flatten() {
            t = t.min(p.deadline.saturating_add(1));
        }
        for st in self.streams.iter().flatten() {
            if let Some(SubProtection::E2e32 { deadline, .. }) =
                self.subs.iter().flatten().find(|s| s.subject == st.subject).map(|s| s.protection)
            {
                t = t.min(st.last_rx.saturating_add(deadline).saturating_add(1));
            }
        }
        t
    }

    /// 某個 `protected` 資料流目前的狀態。
    pub fn stream_state(&self, subject: SubjectId, source: NodeId) -> Option<StreamState> {
        self.streams.iter().flatten().find(|s| s.subject == subject && s.source == source).map(|s| s.sm.state())
    }
}
