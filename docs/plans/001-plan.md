# 001 計畫：設計取向、架構與里程碑

狀態：草案，待審查。術語依 `CONTEXT.md`。

## 一、要回答的問題

一個嵌入式系統裡常同時有 CAN、UART、乙太網路、SPI、I²C 和 Modbus。我們想讓應用程式只面對
Subject（發佈與訂閱）和 Service（一問一答），不管資料實際走哪條線。要用數字回答的是：

- 同一條匯流排上直接互通的延遲，以及經過一次 Bridge 的延遲，各離物理下限多遠。
- Node 程式庫與 Bridge 在 `thumbv7em-none-eabihf` 上佔多少 Flash 與 RAM。
- 一條 Link 滿載或故障時，其他 Link 受多少影響。
- 畸形輸入與通訊故障下的行為是否可預期（見 `002`）。

## 二、設計取向：參考 Cyphal

### 2.1 為什麼選 Cyphal 當藍本

Cyphal 是為車載與航空的內部匯流排設計的 pub/sub 協定。它的出發點和我們相同：資源有限、要求即時、
匯流排種類多。它有幾個做法直接對應我們的需求：

| Cyphal 的做法 | 對我們的好處 |
|---|---|
| 去中心化，同一條匯流排上的 Node 直接收發 | 同匯流排的通訊不多一跳，也沒有中央節點的單點故障 |
| Subject-ID 與 Service-ID 是數字，由系統整合者在設計期指定 | 線上不送名稱，CAN 的 8 B 也放得下；路由可以在建置期算好 |
| 型別固定，由型別定義語言描述 | 收發兩端在建置期就能比對型別 |
| Transfer 模型：多框傳輸靠每框最後的 Tail byte 標示開始、結束與 Transfer-ID，最後一框前放 CRC-16 | 分片、重組與去重複有成熟做法可循 |
| Priority 直接放進 CAN ID | 優先序由 CAN 硬體仲裁保證，不靠軟體排程 |
| 每個 Node 定期發 Heartbeat | 存活偵測與健康狀態有統一機制 |

### 2.2 不相容的部分與原因

我們借設計、不追相容，原因和差異如下：

| 項目 | Cyphal | embnet-rs | 原因 |
|---|---|---|---|
| 跨傳輸轉送 | 沒有標準元件，只有概念驗證 | Bridge 是正式元件，也是 Trust zone 的邊界 | 這是本專案的主要工作 |
| SPI、I²C | 未定義 | Polled link 加 Mailbox | 這兩種介面上 Node 無法主動發話 |
| Modbus 等既有設備 | 不在範圍 | Bridge 上的 Adapter 依設定翻譯 | 現場常有不能改韌體的設備 |
| 認證 | 協定層未定義 | E2E 保護可選擇用截短 MAC 取代 CRC | 見 `002` 第四節 |
| Node-ID | 各傳輸的範圍不同（Cyphal/CAN 最多 127） | 全系統共用一個空間 | 跨傳輸轉送時不用做位址轉換 |

不相容的代價是沒有現成的工具可以當對照組，例如 pycyphal、yakut。所以規格要附 test vector，
並由獨立的非 Rust 程式產生至少一份（見 `CLAUDE.md` 硬規則）。

## 三、四類介面的差異

這四類介面不在同一層，存取模型也不同，所以 Bridge 對每一類要做的事不一樣。

| 介面 | 誰能主動發話 | 單框 payload | 鏈路自帶完整性檢查 | Bridge 要做的事 |
|---|---|---|---|---|
| CAN / CAN FD | 任何 Node，按 ID 仲裁 | 8 B（classic）、64 B（FD） | 有 CRC | 本身就是主要傳輸，Bridge 只在跨 Link 時介入 |
| UART / RS-485 | 點對點雙方都可以；RS-485 多點時要仲裁或輪詢 | 由 framing 決定 | 無，靠 framing 的 CRC（HDLC） | framing；多點時排程 |
| Ethernet / UDP | 任何 Node | 約 1472 B | 有 FCS 與 UDP checksum | 對外 Trust zone 的邊界 |
| SPI / I²C | 只有主端 | 由交易長度決定 | **沒有** | 當主端輪詢 Mailbox，或由 Node 拉中斷線通知 |
| Modbus RTU | 只有主端 | 一次最多讀 125 個暫存器（250 B） | 有 CRC-16 | Adapter 輪詢，把暫存器對應成 Subject |

由這張表推出三個約束：

- **線格式要放得進 CAN classic 的 8 B。** 沿用 Cyphal 的做法，ID 與優先序放在 CAN ID，payload 只放資料和 Tail byte。
- **Polled link 的延遲下限是輪詢週期。** Node 有中斷線時可以縮短。
- **SPI、I²C 沒有鏈路 CRC，完整性要靠 E2E 保護。**

## 四、Wire time：延遲的物理下限

下表依速率與框長推算，不是量測值。用途是判斷 Bridge 處理時間該壓到什麼量級。

| Link | 條件 | 推算 |
|---|---|---|
| CAN classic | 1 Mbit/s、29 位元 ID、8 B 資料 | 不含填充位元 131 bit，約 0.13 ms；每秒最多約 7,600 框 |
| UART | 115200 baud、8N1（每 byte 10 bit） | 約 86.8 µs/byte；16 B 的框約 1.39 ms |
| Ethernet | 100 Mbit/s、最小框（含前導碼與框間隔共 84 B） | 約 6.7 µs |
| I²C | 400 kHz（每 byte 9 bit） | 約 22.5 µs/byte |
| Modbus RTU | 19200 baud、8E1（每字元 11 bit） | 約 573 µs/字元；框間靜默 3.5 字元約 2 ms |

慢速 Link 的 Wire time 以毫秒計，乙太網路是微秒級。Bridge 處理時間應遠小於它服務的最慢 Link 的 Wire time。
延遲報告一律把 Wire time 拆開列，才看得出 Bridge 本身貢獻多少。

## 五、架構草案

P1 定案前，這一節都是草案。

```
   CAN 匯流排：Node A ⇄ Node B        （同匯流排直接互通，不經 Bridge）
        │
     Bridge ──── Ethernet/UDP ──── Linux 主機          （對外 Trust zone）
        ├── UART/HDLC ──────────── Node C
        ├── SPI（輪詢 Mailbox）─── Node D
        └── Modbus RTU（Adapter）─ Legacy device E
```

### 5.1 CAN ID 配置（草案）

以 29 位元延伸 ID 為準：

| Transfer 種類 | 欄位與位元數 | 合計 | 剩餘 |
|---|---|---|---|
| Subject | Priority 3、種類 1、Subject-ID 13、來源 Node-ID 7 | 24 | 5 |
| Service | Priority 3、種類 1、request/response 1、Service-ID 9、目的 Node-ID 7、來源 Node-ID 7 | 28 | 1 |

剩餘位元的用途在 `002` 第四節討論。Node-ID 用 7 位元，全系統最多 128 個 Node。

### 5.2 模組切法

依「介面窄、內部深」的原則，按功能切：

| crate | 內容 | 對外介面 |
|---|---|---|
| `embnet-core` | 線格式、Transfer 的分片與重組、去重複、E2E 保護。`no_std`、不配置記憶體、`forbid(unsafe_code)` | 編碼與解碼函式，不碰 I/O 與時鐘 |
| `embnet-node` | Native node 端：publish、subscribe、call，以及 Heartbeat | 緩衝由呼叫者持有 |
| `embnet-bridge` | 轉送表、Trust zone 檢查、限流、每條出口的有界佇列、Adapter | `on_frame(link, now, bytes)` 與 `poll(now)`，兩者都回傳要送出的 Frame |
| `embnet-config` | 從設定檔產生 ID、型別雜湊、轉送表與 Adapter 對應表，並在建置期驗證 | 建置期工具，輸出常數表 |
| `embnet-link-*` | 各 Link 的 framing 與驅動：`can`、`uart`、`udp`、`spi`、`i2c`、`modbus` | 實作同一個 Link trait |

這樣切的原因：

- 核心與 Bridge 不碰 I/O 與時鐘，同一份程式可以跑在 Linux 與 MCU，也能在主機上決定性重播與 fuzz。
- 驅動與 `unsafe` 只在 Link crate，核心維持可稽核。
- 轉送表在建置期固定，RAM 用量建置時就能算出，`002` 的「跨 Trust zone 預設拒絕」也能在建置期檢查。

## 六、既有實作（P0 要量的對象）

| 實作 | 語言 | 已知 | P0 要確認 |
|---|---|---|---|
| canadensis | Rust | CAN/CAN FD、UDP、serial；UDP 與 serial 標為 experimental；`no_std` 下仍需動態配置 | 體積；哪些模組能在不配置記憶體的前提下沿用 |
| emcyphal | Rust | `no_std` 且不配置記憶體；只支援 CAN | 體積；分片與重組的寫法可否借用 |
| libcanard | C | Cyphal/CAN 的參考實作 | 體積，作為 C 端對照數字 |
| cyphal-bridge-and-router | Python | Bridge 與 router 的概念驗證 | 轉送語意怎麼處理，例如 Transfer-ID 與來源 Node-ID |
| Zenoh（zenoh-pico） | C 與 Rust | 集中式 router 架構；serial、UDP；沒找到 CAN 支援 | 作為「集中式 router」對照，量單跳延遲 |

## 七、里程碑

每個里程碑都有一行可 grep 的通過行，附 run 目錄。fuzz 與故障注入從 P2 開始就跟著跑。

| 里程碑 | 內容 | 通過行 |
|---|---|---|
| P0 | 量第六節的實作：體積、配置方式、CAN 直連延遲 | `EMBNET_PRIOR_ART impl=<名稱> no_alloc=<0\|1> text_kb=<N> ram_kb=<N>` |
| P1 | 規格寫進 `docs/spec/`：線格式、CAN ID、Tail byte、E2E 保護，附 test vector；`002` 定案 | 無，這是取捨 |
| P2 | 單一匯流排：兩個 Native node 在 SocketCAN `vcan` 上直接 pub/sub 與 service | `EMBNET_BUS PASS link=can samples=<N> p50_us=<N> p99_us=<N> max_us=<N>` |
| P3 | Bridge：CAN 與 UDP、CAN 與 UART 之間轉送 | `EMBNET_HOP PASS in=<link> out=<link> samples=<N> p50_us=<N> p99_us=<N> max_us=<N>` |
| P4 | Polled link：SPI 或 I²C 的 Mailbox；Modbus Adapter 對模擬從站 | `EMBNET_POLL PASS link=<名稱> period_us=<N> p99_us=<N>` |
| P5 | Node 程式庫與 Bridge 以 `no_std` 跑在 MCU，先模擬器後實板 | `EMBNET_SIZE target=thumbv7em part=<node\|bridge> text_kb=<N> ram_kb=<N>` |
| P6 | `002` 的 security 措施：CAN 上的截短 MAC、Trust zone 邊界、洪泛隔離、fuzz 覆蓋率 | `EMBNET_SEC PASS fuzz_cov=<N>% flood_other_p99_delta_us=<N>` |

## 八、延遲怎麼量

- 每筆 Transfer 帶發送端時戳，接收端算差值。主機上兩端共用同一個單調時鐘；實板上用同一顆 MCU 的兩條 Link 迴送，
  或用邏輯分析儀同時抓兩條線。
- Bridge 處理時間另外量：主機用單調時鐘，Cortex-M 用 DWT 週期計數器。
- 報告格式：p50、p99、max、樣本數，加上 Link 速率、框長與推算的 Wire time。

## 九、被理想化的項目

- 主機上的 `vcan` 與 pty 沒有真實的 Wire time，數字只代表處理時間。真實延遲要等實板。
- 模擬器的虛擬時鐘相對牆鐘會漂移，模擬器上的時序只能相對比較。
- Modbus 模擬從站的回應時間由我們設定，不代表真實設備。

## 十、待審查決定

| 問題 | 為什麼要先決定 |
|---|---|
| MCU 參考板 | STM32F4 的 bxCAN 只支援 classic CAN；CAN FD 要有 FDCAN 的晶片（例如 STM32G4、H7）。也要確認是否有硬體 AES |
| Node-ID 空間 | 草案是全系統 7 位元共用。超過 128 個 Node 就要改 CAN ID 配置 |
| Linux 閘道硬體與核心 | 要不要用 PREEMPT_RT，影響 p99 的量級 |
| Legacy device 的範圍 | 只做 Modbus RTU，或也包含既有的 CAN 設備（例如以 DBC 描述的） |
| SocketCAN `vcan` | 主機要載入 `vcan` 模組，容器要 `CAP_NET_ADMIN`，兩者都是主機設定的變更 |
| 實板與量測設備 | 真實 Wire time 與 p99 要實板，最好還有邏輯分析儀 |
