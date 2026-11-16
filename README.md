# embnet-rs

用 Rust 寫的嵌入式內部網路，設計參考 [Cyphal](https://opencyphal.org/)。

同一條匯流排上的 Node 直接以 Subject（發佈與訂閱）和 Service（一問一答）互通；
不同匯流排之間由 Bridge 轉送，Bridge 同時是信任邊界上的防火牆。
涵蓋 CAN/CAN FD、UART/RS-485、Ethernet/UDP，也補上 Cyphal 沒有定義的 SPI、I²C 與 Modbus。
核心同時跑在 Linux（`std`）與 MCU（`no_std`、不配置記憶體）上。線格式自訂，不與 Cyphal 相容。

**現況：計畫審查中，還沒有程式碼。** 第一步是量既有的 Cyphal 實作（見 `docs/plans/001` 的 P0）。

## 文件

| 路徑 | 內容 |
|---|---|
| `docs/plans/001-plan.md` | 借用 Cyphal 的部分與差異、四類介面、架構草案、里程碑 |
| `docs/plans/002-safety-security.md` | 通訊故障與對策、威脅模型、CAN 上的截短 MAC、驗證方式 |
| `CONTEXT.md` | 術語表 |
| `CLAUDE.md` | 已定案事項與硬規則 |

## 授權

MIT，見 `LICENSE`。
