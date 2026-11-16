//! Heartbeat（`docs/spec/03-node-services.md` 第二節）。

/// Heartbeat 的 Subject-ID。
pub const SUBJECT_ID: u16 = 8190;
/// payload 長度。
pub const LEN: usize = 7;
/// 發送週期，微秒。
pub const PERIOD_US: u64 = 1_000_000;
/// 判定離線的逾時，微秒。
pub const TIMEOUT_US: u64 = 3_000_000;

/// Heartbeat 的內容。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Heartbeat {
    /// 開機後的秒數。
    pub uptime: u32,
    /// 0 正常、1 注意、2 降級、3 故障。
    pub health: u8,
    /// 0 運作、1 初始化、2 維護、3 軟體更新。
    pub mode: u8,
    /// 有未讀的異常紀錄。
    pub fault_log_unread: bool,
    /// 認證功能未啟用。
    pub auth_disabled: bool,
    /// 最差的 CAN 狀態：0 error active、1 error passive、2 曾 bus off 已恢復、3 目前 bus off。
    pub can_state: u8,
    /// 應用自訂。
    pub vendor: u8,
}

impl Heartbeat {
    /// 編成 7 B。超出範圍的欄位只取有效位元。
    pub fn encode(&self) -> [u8; LEN] {
        let u = self.uptime.to_le_bytes();
        let status = (self.health & 0x3)
            | ((self.mode & 0x7) << 2)
            | (u8::from(self.fault_log_unread) << 5)
            | (u8::from(self.auth_disabled) << 6);
        [u[0], u[1], u[2], u[3], status, self.can_state & 0x3, self.vendor]
    }

    /// 解碼。長度不足或保留位元不是 0 時回傳 `None`。
    pub fn decode(p: &[u8]) -> Option<Self> {
        let b: &[u8; LEN] = p.get(..LEN)?.try_into().ok()?;
        let [u0, u1, u2, u3, status, link, vendor] = *b;
        if status & 0x80 != 0 || link & 0xFC != 0 {
            return None;
        }
        Some(Self {
            uptime: u32::from_le_bytes([u0, u1, u2, u3]),
            health: status & 0x3,
            mode: (status >> 2) & 0x7,
            fault_log_unread: status & 0x20 != 0,
            auth_disabled: status & 0x40 != 0,
            can_state: link & 0x3,
            vendor,
        })
    }
}
