# embnet-rs

用 Rust 寫的嵌入式內部網路，設計參考 [Cyphal](https://opencyphal.org/)。

同一條匯流排上的 Node 直接以 Subject（發佈與訂閱）和 Service（一問一答）互通；
不同匯流排之間由 Bridge 轉送，Bridge 同時是信任邊界上的防火牆。
涵蓋 CAN/CAN FD、UART/RS-485、Ethernet/UDP，也補上 Cyphal 沒有定義的 SPI、I²C 與 Modbus。
核心同時跑在 Linux（`std`）與 MCU（`no_std`、不配置記憶體）上。線格式自訂，不與 Cyphal 相容。

**現況：正在建立基礎知識庫，還沒有程式碼。** 進度見 `PLAN.md`。

## 從哪裡開始讀

- 想知道這個專案要做什麼：[001 計畫](docs/plans/001-plan.md) 的第一、二節。
- 想知道會遇到什麼問題：[003 風險評估](docs/plans/003-risk-assessment.md)，第一節是 Bridge 會不會成為瓶頸。
- 不熟嵌入式：知識庫依下表由上而下讀。[Polled link](docs/10-buses/03-polled-links.md) 用到 Subject 等 pub/sub 概念，可以放到 [Cyphal 的設計](docs/20-protocol/01-cyphal-design.md) 之後再讀。
- 遇到不認得的術語：[CONTEXT.md](CONTEXT.md)。

文章裡常見的簡寫：

| 簡寫 | 意思 |
|---|---|
| `001`、`002`、`003` | `docs/plans/` 下的三份計畫：總計畫、Safety 與 Security、風險評估 |
| P0 到 P6 | `001` 第七節的里程碑 |
| K-xx | `003` 的風險編號 |
| 通過行 | 測試或量測結束時印出的一行固定格式結果，例如 `EMBNET_BUS PASS ...`，可以 grep，作為里程碑的驗收依據 |

## 計畫

| 文件 | 內容 |
|---|---|
| [001-plan.md](docs/plans/001-plan.md) | 借用 Cyphal 的部分與差異、四類介面、架構草案、里程碑與各階段的工作 |
| [002-safety-security.md](docs/plans/002-safety-security.md) | 通訊故障與對策、威脅模型、CAN 上的截短 MAC、驗證方式 |
| [003-risk-assessment.md](docs/plans/003-risk-assessment.md) | Bridge 的瓶頸分析與各類風險 |

## 知識庫

文章依 `PLAN.md` 的順序一篇一篇加入，完成的會列在這裡。

| 主題 | 文章 |
|---|---|
| 匯流排與時序 | [CAN：仲裁、填充位元與錯誤侷限](docs/10-buses/01-can-arbitration.md)、[Wire time 與延遲的組成](docs/10-buses/02-wire-time.md)、[Polled link：讓不能主動發話的從端也能發佈](docs/10-buses/03-polled-links.md) |
| 協定設計 | [Cyphal 的設計：去中心化、Subject 與 Service、Transfer](docs/20-protocol/01-cyphal-design.md)、[分片與重組：狀態機、緩衝與 Bridge 的轉送](docs/20-protocol/02-fragmentation.md)、[CRC 與 framing](docs/20-protocol/03-crc-and-framing.md)、[Bridge 的排隊與延遲上限](docs/20-protocol/04-bridge-queueing.md) |
| Safety 與 Security | [通訊故障模型與 E2E 保護](docs/30-safety-security/01-fault-model-e2e.md)、[MAC、新鮮度與重放](docs/30-safety-security/02-mac-freshness.md)、[Trust zone、Bridge 當防火牆與威脅建模](docs/30-safety-security/03-trust-zones.md) |
| Rust 嵌入式實作 | [no_std 與不配置記憶體：緩衝由誰持有](docs/40-rust-embedded/01-no-alloc.md)、[sans-IO：把協定和 I/O、時鐘分開](docs/40-rust-embedded/02-sans-io.md)、[fuzz、性質測試、Miri 與 Kani](docs/40-rust-embedded/03-testing.md)、[延遲分布怎麼量](docs/40-rust-embedded/04-latency-measurement.md) |

## 授權

MIT，見 `LICENSE`。
