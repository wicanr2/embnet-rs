//! E2E-32 保護（`docs/spec/02-e2e.md`）。

use crate::crc::Crc32Autosar;

/// E2E Sync 的 Subject-ID。
pub const SYNC_SUBJECT_ID: u16 = 8189;
/// E2E-32 的開銷：`seq_low` 1 B 加 CRC 4 B。
pub const OVERHEAD: usize = 5;

/// 參與 CRC、但不上線的 Data ID。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataId {
    /// Subject-ID。
    pub subject_id: u16,
    /// 來源 Node-ID。
    pub source: u8,
    /// 型別雜湊。
    pub type_hash: u64,
}

fn crc_of(id: &DataId, seq: u32, data: &[u8]) -> u32 {
    let mut c = Crc32Autosar::new();
    c.update(&id.subject_id.to_le_bytes());
    c.update(&[id.source]);
    c.update(&id.type_hash.to_le_bytes());
    c.update(&seq.to_le_bytes());
    c.update(data);
    c.get()
}

/// 把應用資料包成 E2E-32 的 payload，寫進 `out`，回傳長度；`out` 不夠時回傳 `None`。
pub fn encode(id: &DataId, seq: u32, data: &[u8], out: &mut [u8]) -> Option<usize> {
    let n = data.len().checked_add(OVERHEAD)?;
    let dst = out.get_mut(..n)?;
    let (head, rest) = dst.split_first_mut()?;
    *head = seq.to_le_bytes()[0];
    let (body, crc) = rest.split_at_mut(data.len());
    body.copy_from_slice(data);
    crc.copy_from_slice(&crc_of(id, seq, data).to_be_bytes());
    Some(n)
}

/// 單筆檢查的結果，代碼依規格第四節。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Status {
    /// 下一筆。
    Ok = 0,
    /// 有漏收，但在容許範圍內。
    OkSomeLost = 1,
    /// 與上一筆相同。
    Repeated = 2,
    /// 漏收超過 `max_delta`。
    WrongSequence = 3,
    /// 長度不足或 CRC 不符。
    Error = 4,
    /// 尚未同步。
    NoSync = 5,
    /// 期限內沒有收到任何一筆。
    NoNewData = 6,
}

impl Status {
    /// 是否交給應用。
    pub const fn deliver(self) -> bool {
        matches!(self, Status::Ok | Status::OkSomeLost)
    }
}

/// 一個（Subject-ID、來源）的接收端序號狀態。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeqReceiver {
    synced: bool,
    last: u32,
    max_delta: u8,
}

impl SeqReceiver {
    /// 未同步的狀態。
    pub const fn new(max_delta: u8) -> Self {
        Self { synced: false, last: 0, max_delta }
    }

    /// 是否已同步。
    pub const fn synced(&self) -> bool {
        self.synced
    }

    /// 最後接受的完整序號。
    pub const fn last(&self) -> u32 {
        self.last
    }

    /// 以 Sync 的「下一筆序號」同步。是否採用由呼叫者依規格第五節判斷。
    pub fn sync(&mut self, next_seq: u32) {
        self.last = next_seq.wrapping_sub(1);
        self.synced = true;
    }

    /// 檢查一筆 E2E-32 payload。結果可交給應用時，回傳應用資料。
    pub fn check<'a>(&mut self, id: &DataId, wire: &'a [u8]) -> (Status, Option<&'a [u8]>) {
        self.check_with(wire, 4, |seq, data, trailer| {
            let mut cb = [0u8; 4];
            cb.copy_from_slice(trailer);
            crc_of(id, seq, data) == u32::from_be_bytes(cb)
        })
    }

    /// 規格第四節的判定，尾端的驗證交給 `verify(完整序號, 應用資料, 尾端)`。CRC 與 MAC 共用。
    /// `trailer_len` 是尾端長度：CRC 為 4，MAC 為 4 或 8。
    pub fn check_with<'a>(
        &mut self,
        wire: &'a [u8],
        trailer_len: usize,
        verify: impl Fn(u32, &[u8], &[u8]) -> bool,
    ) -> (Status, Option<&'a [u8]>) {
        if wire.len() < trailer_len.saturating_add(1) {
            return (Status::Error, None);
        }
        if !self.synced {
            return (Status::NoSync, None);
        }
        let Some((&low, rest)) = wire.split_first() else { return (Status::Error, None) };
        let Some((data, trailer)) = rest.split_at_checked(rest.len().saturating_sub(trailer_len)) else {
            return (Status::Error, None);
        };
        let d = low.wrapping_sub(self.last.to_le_bytes()[0]);
        if d == 0 {
            let st = if verify(self.last, data, trailer) { Status::Repeated } else { Status::Error };
            return (st, None);
        }
        let cand = self.last.wrapping_add(u32::from(d));
        if !verify(cand, data, trailer) {
            return (Status::Error, None);
        }
        self.last = cand;
        if d == 1 {
            (Status::Ok, Some(data))
        } else if d <= self.max_delta {
            (Status::OkSomeLost, Some(data))
        } else {
            (Status::WrongSequence, None)
        }
    }
}

/// 接收狀態機的狀態（規格第六節）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamState {
    /// 還沒收到資料。
    NoData,
    /// 有資料，還不能用。
    Init,
    /// 可以用。
    Valid,
    /// 不能用，接收端應進入安全狀態。
    Invalid,
}

/// 狀態機參數。`window` 最大 32。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateParams {
    /// 滑動窗長度。
    pub window: u8,
    /// INIT 到 VALID 需要的「好」數。
    pub min_ok_init: u8,
    /// INIT 容許的「錯」數。
    pub max_err_init: u8,
    /// VALID 容許的「錯」數。
    pub max_err_valid: u8,
    /// INVALID 回到 VALID 需要的「好」數。
    pub min_ok_invalid: u8,
    /// INVALID 回到 VALID 容許的「錯」數。
    pub max_err_invalid: u8,
}

impl Default for StateParams {
    fn default() -> Self {
        Self { window: 8, min_ok_init: 2, max_err_init: 1, max_err_valid: 2, min_ok_invalid: 4, max_err_invalid: 0 }
    }
}

/// 把單筆結果累積成資料流狀態。用兩個位元遮罩當滑動窗，不配置記憶體。
#[derive(Clone, Copy, Debug)]
pub struct StateMachine {
    params: StateParams,
    state: StreamState,
    good: u32,
    err: u32,
}

impl StateMachine {
    /// 從 NODATA 開始。
    pub fn new(params: StateParams) -> Self {
        Self { params, state: StreamState::NoData, good: 0, err: 0 }
    }

    /// 目前狀態。
    pub const fn state(&self) -> StreamState {
        self.state
    }

    /// 放入一筆結果，回傳新的狀態。
    pub fn push(&mut self, s: Status) -> StreamState {
        if self.state == StreamState::NoData {
            if matches!(s, Status::NoSync | Status::NoNewData) {
                return self.state;
            }
            self.state = StreamState::Init;
        }
        if s == Status::Repeated {
            return self.state;
        }
        let w = u32::from(self.params.window.clamp(1, 32));
        let mask = if w == 32 { u32::MAX } else { (1u32 << w).wrapping_sub(1) };
        self.good = (self.good << 1) & mask;
        self.err = (self.err << 1) & mask;
        if s.deliver() {
            self.good |= 1;
        } else {
            self.err |= 1;
        }
        let g = self.good.count_ones();
        let e = self.err.count_ones();
        let p = &self.params;
        self.state = match self.state {
            StreamState::Init if e > u32::from(p.max_err_init) => StreamState::Invalid,
            StreamState::Init if g >= u32::from(p.min_ok_init) => StreamState::Valid,
            StreamState::Valid if e > u32::from(p.max_err_valid) => StreamState::Invalid,
            StreamState::Invalid if g >= u32::from(p.min_ok_invalid) && e <= u32::from(p.max_err_invalid) => {
                StreamState::Valid
            }
            other => other,
        };
        self.state
    }
}

/// 編一筆 E2E Sync 的 payload。條目數超過 255 或 `out` 不夠時回傳 `None`。
pub fn sync_encode(source: u8, entries: &[(u16, u32)], out: &mut [u8]) -> Option<usize> {
    let k = u8::try_from(entries.len()).ok()?;
    let body_len = entries.len().checked_mul(6)?.checked_add(1)?;
    let n = body_len.checked_add(4)?;
    let dst = out.get_mut(..n)?;
    let (body, crc) = dst.split_at_mut(body_len);
    let (count, rest) = body.split_first_mut()?;
    *count = k;
    for ((s, q), chunk) in entries.iter().zip(rest.as_chunks_mut::<6>().0.iter_mut()) {
        let (a, b) = chunk.split_at_mut(2);
        a.copy_from_slice(&s.to_le_bytes());
        b.copy_from_slice(&q.to_le_bytes());
    }
    let id = DataId { subject_id: SYNC_SUBJECT_ID, source, type_hash: 0 };
    let mut c = Crc32Autosar::new();
    c.update(&id.subject_id.to_le_bytes());
    c.update(&[source]);
    c.update(&[0; 8]);
    c.update(body);
    crc.copy_from_slice(&c.get().to_be_bytes());
    Some(n)
}

/// 驗證並解開 E2E Sync，回傳條目的迭代器；格式或 CRC 不符時回傳 `None`。
pub fn sync_decode(source: u8, payload: &[u8]) -> Option<impl Iterator<Item = (u16, u32)> + '_> {
    let (body, crc) = payload.split_at_checked(payload.len().checked_sub(4)?)?;
    let (&k, rest) = body.split_first()?;
    if rest.len() != usize::from(k).checked_mul(6)? {
        return None;
    }
    let mut c = Crc32Autosar::new();
    c.update(&SYNC_SUBJECT_ID.to_le_bytes());
    c.update(&[source]);
    c.update(&[0; 8]);
    c.update(body);
    let mut cb = [0u8; 4];
    cb.copy_from_slice(crc);
    if c.get() != u32::from_be_bytes(cb) {
        return None;
    }
    Some(rest.as_chunks::<6>().0.iter().map(|&[s0, s1, q0, q1, q2, q3]| {
        (u16::from_le_bytes([s0, s1]), u32::from_le_bytes([q0, q1, q2, q3]))
    }))
}
