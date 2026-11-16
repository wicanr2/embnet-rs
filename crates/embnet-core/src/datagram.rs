//! UART 與 UDP 的 datagram 格式（`docs/spec/04-datagram-links.md`）。

use crate::crc::crc32c;

/// 標頭長度。
pub const HEADER_LEN: usize = 8;
/// 標頭加 CRC 的開銷。
pub const OVERHEAD: usize = HEADER_LEN + 4;
/// Subject 的 destination 欄位值。
pub const NO_DESTINATION: u8 = 0xFF;

/// datagram 的標頭欄位。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// 0 到 7。
    pub priority: u8,
    /// true 是 Service。
    pub service: bool,
    /// Service 時 true 是 request。
    pub request: bool,
    /// Subject-ID 或 Service-ID。
    pub port: u16,
    /// 來源 Node-ID。
    pub source: u8,
    /// 目的 Node-ID；Subject 為 [`NO_DESTINATION`]。
    pub destination: u8,
    /// Transfer-ID，模 256。
    pub transfer_id: u8,
}

/// datagram 被丟棄的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatagramError {
    /// 長度小於 12。
    TooShort,
    /// version 不是 0。
    Version,
    /// 保留欄位不是 0，或 Subject 的 destination 不是 0xFF。
    Reserved,
    /// Node-ID 超出 0 到 127。
    NodeId,
    /// CRC-32C 不符。
    Crc,
    /// 輸出緩衝不夠。
    BufferTooSmall,
}

/// 編一個 datagram，寫進 `out`，回傳長度。
pub fn encode(h: &Header, payload: &[u8], out: &mut [u8]) -> Result<usize, DatagramError> {
    let n = payload.len().checked_add(OVERHEAD).ok_or(DatagramError::BufferTooSmall)?;
    let dst = out.get_mut(..n).ok_or(DatagramError::BufferTooSmall)?;
    if h.source > 127 || (h.service && h.destination > 127) {
        return Err(DatagramError::NodeId);
    }
    let flags = (h.priority & 0x7) | (u8::from(h.service) << 3) | (u8::from(h.service && h.request) << 4);
    let port = h.port.to_le_bytes();
    let dest = if h.service { h.destination } else { NO_DESTINATION };
    {
        let (head, rest) = dst.split_at_mut(HEADER_LEN);
        head.copy_from_slice(&[0, flags, port[0], port[1], h.source, dest, h.transfer_id, 0]);
        rest.get_mut(..payload.len()).ok_or(DatagramError::BufferTooSmall)?.copy_from_slice(payload);
    }
    let (body, tail) = dst.split_at_mut(n.saturating_sub(4));
    tail.copy_from_slice(&crc32c(body).to_be_bytes());
    Ok(n)
}

/// 解一個 datagram，回傳標頭與 payload。
pub fn decode(d: &[u8]) -> Result<(Header, &[u8]), DatagramError> {
    if d.len() < OVERHEAD {
        return Err(DatagramError::TooShort);
    }
    let (body, crc) = d.split_at(d.len().saturating_sub(4));
    let mut cb = [0u8; 4];
    cb.copy_from_slice(crc);
    if crc32c(body) != u32::from_be_bytes(cb) {
        return Err(DatagramError::Crc);
    }
    let (head, payload) = body.split_at(HEADER_LEN);
    let [version, flags, p0, p1, source, destination, transfer_id, reserved]: [u8; HEADER_LEN] =
        head.try_into().map_err(|_| DatagramError::TooShort)?;
    if version != 0 {
        return Err(DatagramError::Version);
    }
    let service = flags & 0x08 != 0;
    let request = flags & 0x10 != 0;
    if reserved != 0 || flags & 0xE0 != 0 || (!service && (request || destination != NO_DESTINATION)) {
        return Err(DatagramError::Reserved);
    }
    if source > 127 || (service && destination > 127) {
        return Err(DatagramError::NodeId);
    }
    Ok((
        Header { priority: flags & 0x7, service, request, port: u16::from_le_bytes([p0, p1]), source, destination, transfer_id },
        payload,
    ))
}

/// COBS 編碼並加上分界 0x00，寫進 `out`，回傳長度。
pub fn cobs_frame(datagram: &[u8], out: &mut [u8]) -> Result<usize, DatagramError> {
    let n = cobs::try_encode(datagram, out).map_err(|_| DatagramError::BufferTooSmall)?;
    let slot = out.get_mut(n).ok_or(DatagramError::BufferTooSmall)?;
    *slot = 0;
    Ok(n.saturating_add(1))
}

/// UART 的接收端：逐位元組餵入，遇到分界 0x00 時解出一個 datagram。`N` 是累積緩衝的大小。
#[derive(Clone, Debug)]
pub struct CobsReceiver<const N: usize> {
    buf: [u8; N],
    len: usize,
    overflow: bool,
    out: [u8; N],
}

impl<const N: usize> Default for CobsReceiver<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// COBS 區塊被丟棄的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CobsError {
    /// 超過緩衝。
    Overflow,
    /// 不是合法的 COBS。
    Decode,
}

impl<const N: usize> CobsReceiver<N> {
    /// 空的接收端。
    pub const fn new() -> Self {
        Self { buf: [0; N], len: 0, overflow: false, out: [0; N] }
    }

    /// 餵入一個位元組。遇到分界時回傳解出的 datagram（尚未做 datagram 檢查）；空區塊回傳 `None`。
    pub fn push(&mut self, b: u8) -> Option<Result<&[u8], CobsError>> {
        if b != 0 {
            match self.buf.get_mut(self.len) {
                Some(slot) => {
                    *slot = b;
                    self.len = self.len.saturating_add(1);
                }
                None => self.overflow = true,
            }
            return None;
        }
        let len = core::mem::take(&mut self.len);
        if core::mem::take(&mut self.overflow) {
            return Some(Err(CobsError::Overflow));
        }
        if len == 0 {
            return None;
        }
        let src = self.buf.get(..len)?;
        Some(match cobs::decode(src, &mut self.out) {
            Ok(r) => self.out.get(..r.frame_size()).ok_or(CobsError::Overflow),
            Err(_) => Err(CobsError::Decode),
        })
    }
}
