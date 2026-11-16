# embnet-rs

用 Rust 寫的嵌入式內部網路，設計參考 [Cyphal](https://opencyphal.org/)。

同一條匯流排上的 Node 直接以 Subject（發佈與訂閱）和 Service（一問一答）互通；
不同匯流排之間由 Bridge 轉送，Bridge 同時是信任邊界上的防火牆。
涵蓋 CAN/CAN FD、UART/RS-485、Ethernet/UDP，也補上 Cyphal 沒有定義的 SPI、I²C 與 Modbus。
核心同時跑在 Linux（`std`）與 MCU（`no_std`、不配置記憶體）上。線格式自訂，不與 Cyphal 相容。

**現況：正在建立基礎知識庫，還沒有程式碼。** 進度見 `PLAN.md`。

## 從哪裡開始讀

- 想知道這個專案要做什麼：`docs/plans/001-plan.md` 的第一、二節。
- 想知道會遇到什麼問題：`docs/plans/003-risk-assessment.md`，第一節是 Bridge 會不會成為瓶頸。
- 不熟嵌入式匯流排：從知識庫的「匯流排與時序」讀起。
- 遇到不認得的術語：`CONTEXT.md`。

## 計畫

| 文件 | 內容 |
|---|---|
| `docs/plans/001-plan.md` | 借用 Cyphal 的部分與差異、四類介面、架構草案、里程碑與各階段的工作 |
| `docs/plans/002-safety-security.md` | 通訊故障與對策、威脅模型、CAN 上的截短 MAC、驗證方式 |
| `docs/plans/003-risk-assessment.md` | Bridge 的瓶頸分析與各類風險 |

## 知識庫

文章依 `PLAN.md` 的順序一篇一篇加入，完成的會列在這裡。

| 主題 | 文章 |
|---|---|
| 匯流排與時序 | [CAN：仲裁、填充位元與錯誤侷限](docs/10-buses/01-can-arbitration.md)、[Wire time 與延遲的組成](docs/10-buses/02-wire-time.md)、[Polled link：讓不能主動發話的從端也能發佈](docs/10-buses/03-polled-links.md) |
| 協定設計 | 未開始 |
| Safety 與 Security | 未開始 |
| Rust 嵌入式實作 | 未開始 |

## 授權

MIT，見 `LICENSE`。
