//! 分框（規格第三節）。

use super::tail::Tail;


/// Link 的 MTU。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mtu {
    /// classic CAN，8 B。
    Classic,
    /// CAN FD，64 B。
    Fd,
}

impl Mtu {
    /// 一個 Frame 最多帶幾個位元組（含 Tail byte）。
    pub const fn bytes(self) -> usize {
        match self {
            Mtu::Classic => 8,
            Mtu::Fd => 64,
        }
    }
}

const FD_LENGTHS: [usize; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];

/// 不小於 n 的最小 CAN FD 合法長度；n 大於 64 時回傳 64。
fn fd_round_up(n: usize) -> usize {
    FD_LENGTHS.iter().copied().find(|&v| v >= n).unwrap_or(64)
}

/// 把一筆 Transfer 的 payload 切成 Frame，依序寫進呼叫者的緩衝。不配置記憶體。
///
/// 多框時，邏輯上的串流是「payload、補位、CRC（2 B，big-endian）」。
#[derive(Clone, Debug)]
pub struct Segmenter<'a> {
    payload: &'a [u8],
    mtu: Mtu,
    tid: u8,
    /// 補位的位元組數（只有 CAN FD 才可能不是 0）。
    pad: usize,
    /// 多框時的 CRC；單框時為 None。
    crc: Option<[u8; 2]>,
    /// 串流中已送出的位置。
    pos: usize,
    toggle: bool,
    done: bool,
}

impl<'a> Segmenter<'a> {
    /// 建立分框器。`transfer_id` 只取低 5 位元。
    pub fn new(payload: &'a [u8], mtu: Mtu, transfer_id: u8) -> Self {
        let m = mtu.bytes().saturating_sub(1);
        let (pad, crc) = if payload.len() <= m {
            let pad = match mtu {
                Mtu::Classic => 0,
                Mtu::Fd => fd_round_up(payload.len().saturating_add(1)).saturating_sub(payload.len().saturating_add(1)),
            };
            (pad, None)
        } else {
            let mut pad = 0usize;
            if mtu == Mtu::Fd {
                loop {
                    let total = payload.len().saturating_add(pad).saturating_add(2);
                    let last = match total.checked_rem(m) {
                        Some(0) | None => m,
                        Some(r) => r,
                    };
                    let with_tail = last.saturating_add(1);
                    if fd_round_up(with_tail) == with_tail {
                        break;
                    }
                    pad = pad.saturating_add(1);
                }
            }
            let mut c = crate::crc::Crc16::new();
            c.update(payload);
            for _ in 0..pad {
                c.update(&[0]);
            }
            (pad, Some(c.get().to_be_bytes()))
        };
        Self { payload, mtu, tid: transfer_id & 0x1F, pad, crc, pos: 0, toggle: true, done: false }
    }

    fn stream_len(&self) -> usize {
        let crc_len = if self.crc.is_some() { 2 } else { 0 };
        self.payload.len().saturating_add(self.pad).saturating_add(crc_len)
    }

    fn stream_byte(&self, i: usize) -> u8 {
        if let Some(b) = self.payload.get(i) {
            return *b;
        }
        let j = i.saturating_sub(self.payload.len());
        if j < self.pad {
            return 0;
        }
        let k = j.saturating_sub(self.pad);
        self.crc.and_then(|c| c.get(k).copied()).unwrap_or(0)
    }

    /// 把下一個 Frame 寫進 `out`，回傳長度；沒有下一框時回傳 `None`。
    /// `out` 至少要有 MTU 的長度，否則回傳 `None` 且不前進。
    pub fn next_frame(&mut self, out: &mut [u8]) -> Option<usize> {
        if self.done || out.len() < self.mtu.bytes() {
            return None;
        }
        let m = self.mtu.bytes().saturating_sub(1);
        let total = self.stream_len();
        let sot = self.pos == 0;
        let end = self.pos.saturating_add(m).min(total);
        let n = end.saturating_sub(self.pos);
        for (k, slot) in out.iter_mut().take(n).enumerate() {
            *slot = self.stream_byte(self.pos.saturating_add(k));
        }
        let eot = end >= total;
        let tail = Tail { sot, eot, toggle: self.toggle, transfer_id: self.tid }.encode();
        let slot = out.get_mut(n)?;
        *slot = tail;
        self.pos = end;
        self.toggle = !self.toggle;
        self.done = eot;
        Some(n.saturating_add(1))
    }

    /// 這筆 Transfer 一共幾框。
    pub fn frame_count(&self) -> usize {
        let m = self.mtu.bytes().saturating_sub(1).max(1);
        self.stream_len().div_ceil(m).max(1)
    }
}
