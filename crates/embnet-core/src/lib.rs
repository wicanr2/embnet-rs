//! embnet 的線格式核心，對應 `docs/spec/` 0.2 版。
//!
//! - [`crc`]：CRC-16/IBM-3740、CRC-32/AUTOSAR、CRC-32C、CRC-64/XZ。
//! - [`can`]：CAN ID、Tail byte、分框與重組（`docs/spec/01-can-transport.md`）。
//! - [`e2e`]：E2E-32 保護、接收判定與狀態機（`docs/spec/02-e2e.md`）。
//! - [`heartbeat`]：Heartbeat 的編解碼（`docs/spec/03-node-services.md`）。
//! - [`datagram`]：UART 與 UDP 的 datagram 與 COBS（`docs/spec/04-datagram-links.md`）。
//! - [`mac`]：截短 AES-128-CMAC 與失敗預算（`docs/spec/05-mac.md`）。
//! - [`remap`]：Bridge 的 Transfer-ID 對應（`docs/spec/06-forwarding.md`）。
//!
//! 全部 `no_std`、不配置記憶體、不 panic；緩衝由呼叫者持有。
#![no_std]

pub mod can;
pub mod crc;
pub mod datagram;
pub mod e2e;
pub mod heartbeat;
pub mod mac;
pub mod remap;

/// 單調遞增的時間，單位微秒。時鐘由外層提供（sans-IO）。
pub type Micros = u64;
