# 001 計畫：範圍、架構草案與里程碑

狀態：草案，待審查。術語依 `CONTEXT.md`。

## 一、要回答的問題

一台 Router 能不能把 UART/RS-485、CAN/CAN FD、Ethernet/UDP、SPI/I²C/Modbus 上的 Node 串在一起，
讓應用端只面對 topic pub/sub 與 service？代價要用數字回答：

- 單跳延遲的分布，以及它離 Wire time 這個物理下限多遠。
- Router 與 client 在 `thumbv7em-none-eabihf` 上的體積與靜態 RAM。
- 一條 Link 滿載或故障時，其他 Link 受多少影響。
- 畸形輸入與通訊故障下的行為是否可預期（見 `002`）。

## 二、四類介面的差異

這四類介面不在同一層，存取模型也不同，Router 要對每一類做不同的事。

| 介面 | 層 | 誰能主動發話 | 單框 payload | 鏈路本身的完整性檢查 | Router 要做的事 |
|---|---|---|---|---|---|
| UART / RS-485 | 實體層，framing 自訂 | 點對點雙方都可以；RS-485 多點時要有仲裁或輪詢 | 由 framing 決定 | 無，靠 framing 的 CRC（沿用 HDLC） | framing、多點時排程 |
| CAN / CAN FD | 鏈路層 | 任何 Node，按 ID 仲裁 | 8 B（classic）、64 B（FD） | 有 CRC | 分片與重組、Topic ID 對應 CAN ID 與優先序 |
| Ethernet / UDP | 網路層之上 | 任何 Node | 約 1472 B（MTU 1500 扣 IP 與 UDP 標頭） | 有 FCS 與 UDP checksum | 對外的 Trust zone 邊界（見 `002`） |
| SPI / I²C | 板內序列匯流排 | 只有主端 | 由交易長度決定 | **沒有** | Router 當主端輪詢 Node 的信箱，或由 Node 拉中斷線通知 |
| Modbus RTU | 應用層協定，跑在 RS-485 上 | 只有主端 | 讀暫存器一次最多 125 個（250 B） | 有 CRC-16 | 多半是 Legacy device，由 Adapter 輪詢並把暫存器對應成 topic |

由這張表推得幾個設計約束：

- **線格式要能放進 CAN classic 的 8 B。** 標頭要很短，Topic ID 用數字，名稱留在建置期設定裡。
  長訊息靠分片。CAN classic 能不能扛 `002` 的 E2E 標頭，要在 P1 用數字決定。
- **主從介面沒有「Node 主動發佈」這件事。** Node 的發佈要先放進信箱，等 Router 來讀。
  延遲下限等於輪詢週期，除非 Node 另有中斷線。
- **SPI、I²C 沒有鏈路層 CRC。** 完整性要靠 E2E 標頭。
- **Legacy device 與 Native client 要分開處理。** Modbus 從站或既有 CAN 裝置不會跑我們的 client，只能由 Adapter 依設定翻譯。

## 三、Wire time：延遲的物理下限

以下是依速率與框架長度推算的數字，不是量測值。用來判斷 Router 處理時間該壓到什麼量級。

| Link | 條件 | 推算 |
|---|---|---|
| UART | 115200 baud、8N1（每 byte 10 bit） | 約 86.8 µs/byte；16 B 的框約 1.39 ms |
| CAN classic | 1 Mbit/s、標準 ID、8 B 資料 | 不含填充位元 111 bit，約 0.11 ms；最壞填充下約 0.13 ms |
| Ethernet | 100 Mbit/s、最小框（含前導碼與框間隔共 84 B） | 約 6.7 µs |
| I²C | 400 kHz（每 byte 9 bit） | 約 22.5 µs/byte |
| Modbus RTU | 19200 baud、8E1（每字元 11 bit） | 約 573 µs/字元；框間靜默 3.5 字元約 2 ms |

慢速 Link 的 Wire time 以毫秒計，乙太網路則是微秒級。Router 處理時間應該遠小於它服務的最慢 Link 的 Wire time。
所以延遲報告一律把 Wire time 拆開來列，才看得出 Router 自己貢獻多少。

## 四、既有方案（P0 要量的對象）

| 方案 | 和構想重疊 | 已知限制 | P0 要確認 |
|---|---|---|---|
| Zenoh：Rust 版 router（zenohd）加 C 的 zenoh-pico | 統一的 pub/sub 與 query，有 router。zenoh-pico 支援 serial（COBS）、UDP、TCP | 沒找到 zenoh-pico 支援 CAN 的證據；MCU 端是 C | CAN 支援、MCU 端體積、serial 與 UDP 之間的單跳延遲 |
| Cyphal：canadensis（Rust）、emcyphal（Rust） | 車載與工業取向的 pub/sub 與 service；CAN/CAN FD、UDP、serial | canadensis 的 UDP 與 serial 傳輸標為 experimental，`no_std` 下仍需動態配置；emcyphal 不需配置，但只支援 CAN | 跨傳輸轉送要靠什麼元件、體積、單跳延遲 |

兩者都沒有主從輪詢介面（SPI、I²C、Modbus）的橋接。這可能是本 repo 和既有方案的差別，但要量過才算數。
P0 的結論可能是「採用其中一個，只補 Adapter 與主從介面」，計畫不預設要自寫。

## 五、架構草案

P1 定案前，這一節只是草案。

```
 Native client ── UART/HDLC ──┐
 Native client ── CAN FD ─────┤
 Legacy device ── Modbus RTU ─┤◄─ 輪詢 ─ Adapter ┐
 Native client ── SPI 信箱 ───┤◄─ 輪詢 ─────────┤
                              ▼                  ▼
                       ┌──────────────────────────────┐
                       │ Router 核心（sans-IO、no_std）│
                       │ Routing table（建置期固定）  │
                       └──────────────┬───────────────┘
                                      ▼
                               Ethernet / UDP ── Linux 主機或另一台 Router
```

模組切法，依介面窄、內部深的原則：

| crate | 內容 | 對外介面 |
|---|---|---|
| `embnet-core` | 線格式編解碼、Routing table、session、佇列與限流。`no_std`、不配置記憶體、`forbid(unsafe_code)` | `Router::on_frame(link, now, bytes)`、`Router::poll(now)`，兩者都回傳要送出的框。不碰 I/O，也不碰時鐘 |
| `embnet-client` | Native client 端的 session 與 Endpoint | `publish`、`take`、`call`，緩衝由呼叫者持有 |
| `embnet-config` | 從設定檔產生 Routing table、Topic ID 與 Adapter 對應表，並在建置期驗證 | 建置期工具，輸出常數表 |
| `embnet-link-*` | 各 Link 的 framing 與驅動：`uart`（HDLC）、`can`、`udp`、`spi`、`i2c`、`modbus` | 實作一個 Link trait |

這樣切的原因：

- 核心不碰 I/O 與時鐘，所以同一份程式可以跑在 Linux 與 MCU 上，也能在主機上決定性地重播與 fuzz。
- 驅動與 `unsafe` 只出現在鏈路 crate，核心維持可稽核。
- Routing table 在建置期固定，讓 RAM 用量可在建置時算出，也讓 `002` 的「預設拒絕」能在建置期檢查。

`xrce-rs` 裡已經有 HDLC framing 與傳輸 trait。授權相容的話可以移植過來，但線格式不共用。

## 六、里程碑

每個里程碑都有一行可 grep 的通過行，附 run 目錄。故障注入與 fuzz 從 P2 開始就跟著跑，不留到最後。

| 里程碑 | 內容 | 通過行 |
|---|---|---|
| P0 | 既有方案對照：四類介面的涵蓋、單跳延遲分布、`thumbv7em` 體積、`no_std` 與動態配置 | `EMBNET_PRIOR_ART stack=<名稱> links=<清單> no_alloc=<0\|1> text_kb=<N> hop_p99_us=<N>` |
| P1 | 路線定案寫進 `003`；線格式規格寫進 `docs/spec/`；`002` 從草案改為定案 | 無，這是取捨 |
| P2 | 主機上 `std`：核心 + UART（pty）+ UDP，兩個 Native client 經 Router 完成 pub/sub 與 service | `EMBNET_HOP PASS in=uart out=udp samples=<N> p50_us=<N> p99_us=<N> max_us=<N>` |
| P3 | CAN / CAN FD：先在主機的 SocketCAN `vcan` 上，再上實板 | `EMBNET_HOP PASS in=can out=udp ...`，另列分片數 |
| P4 | 主從介面：Modbus RTU Adapter 對模擬從站；SPI 或 I²C 信箱 | `EMBNET_POLL PASS link=<名稱> period_us=<N> p99_us=<N>` |
| P5 | Router 以 `no_std` 跑在 MCU（先模擬器，後實板）；量體積 | `EMBNET_SIZE target=thumbv7em text_kb=<N> ram_kb=<N>` |
| P6 | `002` 的 security 措施：鏈路認證、Trust zone 邊界、洪泛隔離、fuzz 覆蓋率 | `EMBNET_SEC PASS fuzz_cov=<N>% flood_isolation_p99_delta_us=<N>` |

## 七、延遲怎麼量

- 每筆訊息帶發送端時戳，接收端算差值。主機上兩端共用同一個單調時鐘；實板上用同一顆 MCU 的兩條 Link 迴送，
  或用邏輯分析儀抓兩條線。
- Router 處理時間另外量：主機用單調時鐘，Cortex-M 用 DWT 週期計數器。
- 報告格式：p50、p99、max、樣本數，加上 Link 速率與框長，並另列推算的 Wire time。

## 八、被理想化的項目

- 主機上的 pty 與 `vcan` 沒有真實的 Wire time，延遲數字只能代表 Router 處理時間。真實鏈路的延遲要等實板。
- 模擬器的虛擬時鐘相對牆鐘會漂移，模擬器上的時序只能相對比較。
- Modbus 模擬從站的回應時間由我們設定，不代表真實設備。

## 九、待審查決定

| 問題 | 為什麼要先決定 |
|---|---|
| MCU 參考板 | CAN FD 需要有 FDCAN 的晶片（例如 STM32G4、H7）；STM32F4 的 bxCAN 只支援 classic CAN |
| Linux 閘道硬體與核心 | 要不要用 PREEMPT_RT 影響 p99 的量級 |
| Legacy device 的範圍 | 只做 Modbus RTU，還是也包含既有的 CAN 訊號（例如以 DBC 描述的裝置） |
| SocketCAN `vcan` | 主機要載入 `vcan` 模組，容器需要 `CAP_NET_ADMIN`，這兩項是主機設定的變更 |
| 實板與量測設備 | 真實 Wire time 與 p99 需要實板，最好還有邏輯分析儀 |
| `002` 的安全等級與機密性需求 | 決定 E2E 標頭與 MAC 的長度，進而決定 CAN classic 是否可用 |
