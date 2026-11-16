//! CAN 傳輸（`docs/spec/01-can-transport.md`）。

mod id;
mod rx;
mod tail;
mod tx;

pub use id::{CanId, IdError, NodeId, Priority, ServiceId, SubjectId};
pub use rx::{Accepted, Reassembler, RxError, SessionTable};
pub use tail::Tail;
pub use tx::{Mtu, Segmenter};

/// classic CAN 的最大資料長度。
pub const CLASSIC_MTU: usize = 8;
/// CAN FD 的最大資料長度。
pub const FD_MTU: usize = 64;
