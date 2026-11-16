//! 重組、去重複與逾時（規格第五、六節）。

use super::id::CanId;
use super::tail::Tail;
use crate::Micros;
use crate::crc::Crc16;

/// 收到的 Frame 被丟棄的原因。每一種都要計數（規格第七節）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RxError {
    /// 長度為 0，沒有 Tail byte。
    Empty,
    /// toggle 不符，或 SOT 框的 toggle 不是 1。
    BadToggle,
    /// 非 SOT 框，但沒有對應的進行中狀態，或 Transfer-ID 不同。
    NoSession,
    /// 累積的長度超過緩衝。
    Overflow,
    /// 多框 Transfer 的 CRC 不符。
    Crc,
    /// 逾時內重複收到同一個 Transfer-ID。
    Duplicate,
    /// 重組狀態已用完。
    Exhausted,
}

/// 交給應用的一筆 Transfer。
#[derive(Debug, PartialEq, Eq)]
pub struct Accepted<'a> {
    /// Transfer-ID。
    pub transfer_id: u8,
    /// payload，不含 Tail byte 與 CRC；CAN FD 的補位仍在尾端，由型別的解碼器忽略。
    pub payload: &'a [u8],
}

/// 一個 session 的重組狀態。`N` 是緩衝大小，要不小於「型別最大長度 + 2」。
#[derive(Clone, Debug)]
pub struct Reassembler<const N: usize> {
    active: bool,
    tid: u8,
    next_toggle: bool,
    len: usize,
    started: Micros,
    last_tid: Option<u8>,
    last_time: Micros,
    buf: [u8; N],
}

impl<const N: usize> Default for Reassembler<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Reassembler<N> {
    /// 空的狀態。
    pub const fn new() -> Self {
        Self { active: false, tid: 0, next_toggle: false, len: 0, started: 0, last_tid: None, last_time: 0, buf: [0; N] }
    }

    /// 是否有進行中的多框 Transfer。
    pub const fn is_active(&self) -> bool {
        self.active
    }

    /// 清除逾時的進行中狀態。回傳是否清除了。
    pub fn handle_timeout(&mut self, now: Micros, timeout: Micros) -> bool {
        if self.active && now.saturating_sub(self.started) > timeout {
            self.active = false;
            return true;
        }
        false
    }

    /// 這個 session 是否可以被別的 session 取代：沒有進行中的狀態，去重複的紀錄也已過期。
    fn reusable(&self, now: Micros, timeout: Micros) -> bool {
        !self.active && (self.last_tid.is_none() || now.saturating_sub(self.last_time) >= timeout)
    }

    fn append(&mut self, body: &[u8]) -> Result<(), RxError> {
        let end = self.len.checked_add(body.len()).ok_or(RxError::Overflow)?;
        let Some(dst) = self.buf.get_mut(self.len..end) else {
            self.active = false;
            return Err(RxError::Overflow);
        };
        dst.copy_from_slice(body);
        self.len = end;
        Ok(())
    }

    fn dedup(&mut self, now: Micros, timeout: Micros, tid: u8) -> Result<(), RxError> {
        if self.last_tid == Some(tid) && now.saturating_sub(self.last_time) < timeout {
            return Err(RxError::Duplicate);
        }
        self.last_tid = Some(tid);
        self.last_time = now;
        Ok(())
    }

    /// 處理一個 Frame 的資料（含 Tail byte）。完成一筆 Transfer 時回傳它。
    pub fn push(&mut self, now: Micros, timeout: Micros, data: &[u8]) -> Result<Option<Accepted<'_>>, RxError> {
        let (&tail_byte, body) = data.split_last().ok_or(RxError::Empty)?;
        let tail = Tail::decode(tail_byte);
        self.handle_timeout(now, timeout);

        if tail.sot {
            if !tail.toggle {
                return Err(RxError::BadToggle);
            }
            self.active = true;
            self.tid = tail.transfer_id;
            self.next_toggle = false;
            self.len = 0;
            self.started = now;
            self.append(body)?;
            if !tail.eot {
                return Ok(None);
            }
            // 單框
            self.active = false;
            self.dedup(now, timeout, tail.transfer_id)?;
            let payload = self.buf.get(..self.len).ok_or(RxError::Overflow)?;
            return Ok(Some(Accepted { transfer_id: tail.transfer_id, payload }));
        }

        if !self.active || tail.transfer_id != self.tid {
            return Err(RxError::NoSession);
        }
        if tail.toggle != self.next_toggle {
            return Err(RxError::BadToggle);
        }
        self.append(body)?;
        self.next_toggle = !self.next_toggle;
        if !tail.eot {
            return Ok(None);
        }

        self.active = false;
        let all = self.buf.get(..self.len).ok_or(RxError::Overflow)?;
        let mut c = Crc16::new();
        c.update(all);
        if c.get() != 0 || self.len < 2 {
            return Err(RxError::Crc);
        }
        let tid = self.tid;
        self.dedup(now, timeout, tid)?;
        let payload = self.buf.get(..self.len.saturating_sub(2)).ok_or(RxError::Overflow)?;
        Ok(Some(Accepted { transfer_id: tid, payload }))
    }
}

/// session 的鍵：CAN ID 去掉 Priority。
fn session_key(id: CanId) -> u32 {
    id.encode() & !(0x7 << 26)
}

/// 固定容量的 session 表。`S` 是 session 數，`N` 是每個 session 的緩衝大小。
#[derive(Clone, Debug)]
pub struct SessionTable<const S: usize, const N: usize> {
    keys: [Option<u32>; S],
    slots: [Reassembler<N>; S],
}

impl<const S: usize, const N: usize> Default for SessionTable<S, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const S: usize, const N: usize> SessionTable<S, N> {
    /// 空表。
    pub const fn new() -> Self {
        Self { keys: [None; S], slots: [const { Reassembler::new() }; S] }
    }

    /// 處理一個 Frame。找不到 session 時取一個空的或可重用的位置；都沒有就回傳 [`RxError::Exhausted`]。
    pub fn push(&mut self, now: Micros, timeout: Micros, id: CanId, data: &[u8]) -> Result<Option<Accepted<'_>>, RxError> {
        let key = session_key(id);
        let idx = match self.keys.iter().position(|k| *k == Some(key)) {
            Some(i) => i,
            None => {
                let free = self.keys.iter().zip(self.slots.iter()).position(|(k, s)| k.is_none() || s.reusable(now, timeout));
                let i = free.ok_or(RxError::Exhausted)?;
                if let (Some(k), Some(s)) = (self.keys.get_mut(i), self.slots.get_mut(i)) {
                    *k = Some(key);
                    *s = Reassembler::new();
                }
                i
            }
        };
        self.slots.get_mut(idx).ok_or(RxError::Exhausted)?.push(now, timeout, data)
    }

    /// 清除所有逾時的進行中狀態，回傳清除的數量。
    pub fn handle_timeout(&mut self, now: Micros, timeout: Micros) -> usize {
        self.slots.iter_mut().filter_map(|s| s.handle_timeout(now, timeout).then_some(())).count()
    }
}
