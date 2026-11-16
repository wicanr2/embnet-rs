# embnet-rs

用 Rust 寫的嵌入式內部網路。一台 Router 串接 UART/RS-485、CAN/CAN FD、Ethernet/UDP、
SPI/I²C/Modbus 上的裝置，對應用端提供同一套 topic pub/sub 與 service 介面。
Router 的核心同時跑在 Linux（`std`）與 MCU（`no_std`、不配置記憶體）上。

架構借用 XRCE-DDS 的 client-agent 概念：資源有限的裝置只保留最少狀態，路由與 session 交給 Router。
線格式自訂，不與 ROS 2 互通。

**現況：計畫審查中，還沒有程式碼。** 第一步是對照既有方案 Zenoh 與 Cyphal（見 `docs/plans/001` 的 P0）。

## 文件

| 路徑 | 內容 |
|---|---|
| `docs/plans/001-plan.md` | 範圍、四類介面的差異、架構草案、里程碑 |
| `docs/plans/002-safety-security.md` | 通訊故障與對策、威脅模型、驗證方式 |
| `CONTEXT.md` | 術語表 |
| `CLAUDE.md` | 已定案事項與硬規則 |

## 授權

MIT，見 `LICENSE`。
