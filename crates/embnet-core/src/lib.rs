//! embnet 的線格式核心，對應 `docs/spec/` 0.1 版。
//!
//! - [`crc`]：CRC-16/IBM-3740 與 CRC-32/AUTOSAR。
//! - [`can`]：CAN ID、Tail byte、分框與重組（`docs/spec/01-can-transport.md`）。
//! - [`e2e`]：E2E-32 保護、接收判定與狀態機（`docs/spec/02-e2e.md`）。
//! - [`heartbeat`]：Heartbeat 的編解碼（`docs/spec/03-node-services.md`）。
//!
//! 全部 `no_std`、不配置記憶體、不 panic；緩衝由呼叫者持有。
#![no_std]

pub mod can;
pub mod crc;
pub mod e2e;
pub mod heartbeat;

/// 單調遞增的時間，單位微秒。時鐘由外層提供（sans-IO）。
pub type Micros = u64;
