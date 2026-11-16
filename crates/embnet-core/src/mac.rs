//! 截短 AES-128-CMAC 與失敗預算（`docs/spec/05-mac.md`）。

use crate::Micros;
use crate::e2e::{DataId, SeqReceiver, Status};
use aes::Aes128;
use cmac::{Cmac, KeyInit, Mac};
use subtle::ConstantTimeEq;

/// tag 長度。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagLen {
    /// 32 位元，classic CAN 用。
    Bits32,
    /// 64 位元，CAN FD 與 UDP 用。
    Bits64,
}

impl TagLen {
    /// 位元組數。
    pub const fn bytes(self) -> usize {
        match self {
            TagLen::Bits32 => 4,
            TagLen::Bits64 => 8,
        }
    }

    /// 風險取 2⁻²⁰ 時，每把金鑰的失敗預算。
    pub const fn budget(self) -> u64 {
        match self {
            TagLen::Bits32 => 1 << 12,
            TagLen::Bits64 => 1 << 44,
        }
    }
}

/// 128 位元金鑰。內容由平台提供。
pub type Key = [u8; 16];

/// 完整的 16 B CMAC。輸入與 E2E CRC 相同。
pub fn cmac_full(key: &Key, id: &DataId, seq: u32, data: &[u8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    let Ok(mut m) = <Cmac<Aes128> as KeyInit>::new_from_slice(key) else { return out };
    m.update(&id.subject_id.to_le_bytes());
    m.update(&[id.source]);
    m.update(&id.type_hash.to_le_bytes());
    m.update(&seq.to_le_bytes());
    m.update(data);
    out.copy_from_slice(&m.finalize().into_bytes());
    out
}

/// 包成 `mac32` 或 `mac64` 的 payload，寫進 `out`，回傳長度。
pub fn encode(key: &Key, tag: TagLen, id: &DataId, seq: u32, data: &[u8], out: &mut [u8]) -> Option<usize> {
    let n = data.len().checked_add(1)?.checked_add(tag.bytes())?;
    let dst = out.get_mut(..n)?;
    let (head, rest) = dst.split_first_mut()?;
    *head = seq.to_le_bytes()[0];
    let (body, t) = rest.split_at_mut(data.len());
    body.copy_from_slice(data);
    t.copy_from_slice(cmac_full(key, id, seq, data).get(..tag.bytes())?);
    Some(n)
}

/// 以 MAC 驗證一筆 payload；規則同 E2E-32，只把 CRC 換成常數時間的 tag 比對。
pub fn check<'a>(rx: &mut SeqReceiver, key: &Key, tag: TagLen, id: &DataId, wire: &'a [u8]) -> (Status, Option<&'a [u8]>) {
    rx.check_with(wire, tag.bytes(), |seq, data, t| {
        cmac_full(key, id, seq, data).get(..t.len()).is_some_and(|full| bool::from(full.ct_eq(t)))
    })
}

/// 一把金鑰的失敗預算（規則 1）。
#[derive(Clone, Copy, Debug)]
pub struct KeyBudget {
    used: u64,
    budget: u64,
}

impl KeyBudget {
    /// 依 tag 長度取預算。
    pub const fn new(tag: TagLen) -> Self {
        Self { used: 0, budget: tag.budget() }
    }

    /// 記一次失敗。回傳預算是否已用完。
    pub fn record_failure(&mut self) -> bool {
        self.used = self.used.saturating_add(1);
        self.exhausted()
    }

    /// 預算是否已用完；用完後這把金鑰的結果一律為 ERROR，直到換金鑰。
    pub const fn exhausted(&self) -> bool {
        self.used >= self.budget
    }

    /// 已用的次數。
    pub const fn used(&self) -> u64 {
        self.used
    }
}

/// 一個（來源、Link）的失敗隔離（規則 2）。
#[derive(Clone, Copy, Debug)]
pub struct SourceGuard {
    window_start: Micros,
    count: u32,
    quarantined_until: Option<Micros>,
}

/// 隔離的參數。
#[derive(Clone, Copy, Debug)]
pub struct GuardParams {
    /// 時間窗，微秒。
    pub window_us: Micros,
    /// 時間窗內容許的失敗數。
    pub fail_burst: u32,
    /// 隔離時間，微秒。
    pub quarantine_us: Micros,
}

impl Default for GuardParams {
    fn default() -> Self {
        Self { window_us: 60_000_000, fail_burst: 8, quarantine_us: 60_000_000 }
    }
}

impl Default for SourceGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceGuard {
    /// 未隔離。
    pub const fn new() -> Self {
        Self { window_start: 0, count: 0, quarantined_until: None }
    }

    /// 這個來源目前是否被隔離；隔離中的框直接丟棄，不驗證也不計入金鑰的預算。
    pub fn quarantined(&mut self, now: Micros) -> bool {
        match self.quarantined_until {
            Some(t) if now < t => true,
            Some(_) => {
                self.quarantined_until = None;
                self.count = 0;
                self.window_start = now;
                false
            }
            None => false,
        }
    }

    /// 記一次失敗。回傳是否因此進入隔離。
    pub fn record_failure(&mut self, now: Micros, p: &GuardParams) -> bool {
        if now.saturating_sub(self.window_start) >= p.window_us {
            self.window_start = now;
            self.count = 0;
        }
        self.count = self.count.saturating_add(1);
        if self.count > p.fail_burst {
            self.quarantined_until = Some(now.saturating_add(p.quarantine_us));
            return true;
        }
        false
    }
}
