//! CAN ID 的配置（規格第一節）。

/// 優先等級，0 最高、7 最低。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Priority(u8);
/// Node-ID，0 到 127。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u8);
/// Subject-ID，0 到 8191。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubjectId(u16);
/// Service-ID，0 到 511。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ServiceId(u16);

macro_rules! bounded {
    ($t:ident, $inner:ty, $max:expr) => {
        impl $t {
            /// 最大值。
            pub const MAX: $inner = $max;
            /// 超出範圍時回傳 `None`。
            pub const fn new(v: $inner) -> Option<Self> {
                if v <= $max { Some(Self(v)) } else { None }
            }
            /// 取出數值。
            pub const fn get(self) -> $inner {
                self.0
            }
        }
    };
}
bounded!(Priority, u8, 7);
bounded!(NodeId, u8, 127);
bounded!(SubjectId, u16, 8191);
bounded!(ServiceId, u16, 511);

/// 解碼後的 CAN ID。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CanId {
    /// Subject 的發佈。
    Subject {
        /// 優先等級。
        priority: Priority,
        /// Subject-ID。
        subject: SubjectId,
        /// 來源。
        source: NodeId,
    },
    /// Service 的 request 或 response。
    Service {
        /// 優先等級。
        priority: Priority,
        /// true 是 request。
        request: bool,
        /// Service-ID。
        service: ServiceId,
        /// 目的。
        destination: NodeId,
        /// 來源。
        source: NodeId,
    },
}

/// CAN ID 解碼失敗的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdError {
    /// 超出 29 位元。
    TooWide,
    /// 保留位元不是 0。
    Reserved,
}

impl CanId {
    /// 編成 29 位元的延伸 ID。
    pub fn encode(self) -> u32 {
        match self {
            CanId::Subject { priority, subject, source } => {
                (u32::from(priority.0) << 26) | (u32::from(subject.0) << 7) | u32::from(source.0)
            }
            CanId::Service { priority, request, service, destination, source } => {
                (u32::from(priority.0) << 26)
                    | (1 << 25)
                    | (u32::from(request) << 24)
                    | (u32::from(service.0) << 14)
                    | (u32::from(destination.0) << 7)
                    | u32::from(source.0)
            }
        }
    }

    /// 從 29 位元的延伸 ID 解碼。保留位元不是 0 時回傳錯誤。
    pub fn decode(raw: u32) -> Result<Self, IdError> {
        if raw >> 29 != 0 {
            return Err(IdError::TooWide);
        }
        let priority = Priority(((raw >> 26) & 0x7) as u8);
        let source = NodeId((raw & 0x7F) as u8);
        if raw & (1 << 25) == 0 {
            if (raw >> 20) & 0x1F != 0 {
                return Err(IdError::Reserved);
            }
            Ok(CanId::Subject { priority, subject: SubjectId(((raw >> 7) & 0x1FFF) as u16), source })
        } else {
            if raw & (1 << 23) != 0 {
                return Err(IdError::Reserved);
            }
            Ok(CanId::Service {
                priority,
                request: raw & (1 << 24) != 0,
                service: ServiceId(((raw >> 14) & 0x1FF) as u16),
                destination: NodeId(((raw >> 7) & 0x7F) as u8),
                source,
            })
        }
    }
}
