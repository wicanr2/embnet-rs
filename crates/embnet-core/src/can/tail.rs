//! Tail byte（規格第二節）。

/// 每個 Frame 最後一個位元組的內容。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tail {
    /// 第一框。
    pub sot: bool,
    /// 最後一框。
    pub eot: bool,
    /// 第一框為 true，之後逐框交替。
    pub toggle: bool,
    /// Transfer-ID，0 到 31。
    pub transfer_id: u8,
}

impl Tail {
    /// 編成一個位元組。Transfer-ID 只取低 5 位元。
    pub fn encode(self) -> u8 {
        (u8::from(self.sot) << 7) | (u8::from(self.eot) << 6) | (u8::from(self.toggle) << 5) | (self.transfer_id & 0x1F)
    }

    /// 從一個位元組解碼。
    pub fn decode(b: u8) -> Self {
        Self { sot: b & 0x80 != 0, eot: b & 0x40 != 0, toggle: b & 0x20 != 0, transfer_id: b & 0x1F }
    }
}
