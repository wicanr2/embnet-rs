# 003 風險評估

狀態：草案，待審查。術語依 `CONTEXT.md`。編號 `K-xx` 供 `001` 的里程碑引用。

每項風險寫明成因、最早在哪裡看得出來，以及怎麼降低。看得出來的時間點愈早，代價愈小，
所以多數風險都安排在 P0 或 P1 用計算或小實驗先確認。

## 一、Bridge 會不會變成瓶頸

改成邊界 Bridge 之後，同匯流排的流量不經過它。但跨匯流排的流量仍然全部經過 Bridge，
而嵌入式系統常見的流量型態正好是「多條匯流排的感測資料匯集到一台主機」。所以 Bridge 仍可能是瓶頸，
只是要看瓶頸卡在哪一種資源。以下逐項算。

### 1.1 出口 Link 的容量

這一項與 Bridge 的 CPU 無關，是物理限制。Bridge 從快的 Link 轉進慢的 Link 時，
只要要轉送的流量超過慢 Link 的容量，佇列就一定會滿，CPU 再快都沒用。

| 方向 | 例子 | 推算 |
|---|---|---|
| 快轉慢 | Linux 主機經 100 Mbit/s 乙太網路送指令到 1 Mbit/s 的 CAN | 速率差約 100 倍。主機只要稍微多發，CAN 側的佇列就會滿 |
| 慢轉快（匯集） | 4 條 CAN 匯流排的資料都轉到 UDP | 8 B 框時每條 CAN 最多約 7,600 框/s，合計約 30,500 框/s。若每個 CAN 框都包成一個 UDP 封包，乙太網路上每框至少 84 B，約 20.5 Mbit/s，100 Mbit/s 仍有餘裕 |

排隊的直覺：出口使用率 ρ 接近 1 時，等待時間會急遽上升。以最簡單的單一佇列模型
（M/M/1）來看，平均等待約為服務時間的 ρ/(1−ρ) 倍：ρ = 0.5 時約 1 倍，ρ = 0.9 時約 9 倍。
實際流量多半是週期性的，不完全符合這個模型，但趨勢相同，而 p99 惡化得比平均更快。

對策：

- **建置期的流量預算。** `embnet-config` 依每個 Subject 的發佈週期與大小，算出每條出口 Link 的使用率，
  超過設定上限就拒絕建置。這把瓶頸從執行期的意外變成建置期的錯誤。
- **入口限流。** 從快 Link 進來、要轉進慢 Link 的 Subject，在 Bridge 入口依預算限流。
- **出口依 Priority 排序。** 出口佇列滿時先犧牲低優先序，高優先序的延遲不受低優先序流量拖累。
- **匯集方向打包。** 轉往 UDP 時，把同一時段的多個 Transfer 放進同一個 UDP 封包，減少封包數。

### 1.2 Bridge 的 CPU

以 STM32F427（最高 180 MHz，有兩個 bxCAN）當 Bridge、兩條 classic CAN 都滿載來計算。最壞的框率出現在最短的框：
Cyphal 式的框至少帶 1 B（Tail byte），1 B 資料的延伸框不含填充是 75 bit（第 1.3 節）。

- 每條 1 Mbit/s 的 CAN 最多約 13,333 框/s，兩條合計約 26,667 框/s，每框可用約 6,750 個時脈週期（CPU 全滿時）。
  保留一半餘裕，就剩約 3,375 個週期，而且這還沒算 UART、SPI 與 UDP 的流量。
- 每框要做的事：解析、查轉送表、檢查 E2E、必要時重組或重新分片、放入出口佇列。
- 驗證或產生 MAC：STM32F427 沒有硬體 AES，只能用軟體。MAC 的輸入包含 Data ID、Type hash、完整 FV 與 payload（`002` 第 4.4 節），
  通常超過 16 B，要算 2 個 AES 區塊。公開數字：STM32F407 上查表式 AES-128 一個區塊約 662 個週期；常數時間的 bitsliced 版以 CTR 模式計約 1,618 個，
  但那是兩個區塊並行處理的攤提值，CMAC 是串接計算，單一區塊的成本可能接近兩倍（Schwabe & Stoffelen 2016，待實測）。
  合計：查表式約 1,300 個週期以上，常數時間版可能超過 3,000 個，等於吃掉大部分甚至全部的每框預算。
- 一筆進來、要轉往多條 Link 的 Transfer（fan-out），工作量按出口數倍增。

結論：P1 實測之前，不能判斷 STM32F427 能否在兩條滿載 CAN 上做常數時間的 MAC 驗證。

對策：

- P1 用 DWT 週期計數器量每一步的週期數，算出這顆晶片能扛的最大框率，寫進規格。
- 格式相同的 Link 之間（例如 CAN 轉 CAN），不重組也不重新分片，直接轉送單框。
- 只有跨 Trust zone 的 `protected` Subject 才在 Bridge 驗證 MAC，同一 zone 內的 MAC 由收發兩端自己驗證。
- 負載超過一台 MCU 能扛的量時，改用有硬體 AES 的晶片（例如 F437、F439）或 Linux 當 Bridge，或拆成多台 Bridge（見 1.5）。

### 1.3 接收中斷與硬體 FIFO

這是 MCU 當 Bridge 時最容易被忽略的瓶頸。依 STM32F4 參考手冊（RM0090），bxCAN 有兩個接收 FIFO，每個 3 格；
STM32G4 的 FDCAN 也是兩個 FIFO 各 3 格，H7 才可以配置到每個 64 格（細節見 `docs/10-buses/01` 第七節）。
1 Mbit/s 下，帶 1 B 資料的延伸框約 75 bit，約 75 µs 就來一框。全部框都進同一個 FIFO 時，
Bridge 必須在約 225 µs 內把 FIFO 讀走，否則就會溢位掉框。這個時間包含其他中斷與關中斷的臨界區。
溢位掉的框，控制器已經回了 ACK，發送端不會重送。

對策：

- 接收中斷只把框複製進 RAM 的環形緩衝區，不在中斷裡解析、驗 MAC 或轉送。
- 用 DMA 搬運 SPI、UART 的資料。
- P5 量最壞中斷延遲，並統計 FIFO 溢位次數（bxCAN 有溢位旗標）。

### 1.4 Polled link 佔用的時間

在 SPI、I²C、Modbus 上，Bridge 是主端，所有交易都由它發起，所以這些 Link 上的流量一定經過 Bridge。

- Modbus RTU 是半雙工。一個從站回應慢或沒回應，整條 RS-485 都要等到逾時才能問下一個。19200 baud 下，
  框間靜默就約 2 ms，加上回應逾時，一輪輪詢很容易拉到數十毫秒。
- 輪詢週期愈短，延遲愈低，但空問（Mailbox 是空的）也愈多，浪費匯流排和 CPU。

對策：Native node 拉中斷線通知 Bridge 有資料；依 Subject 的期限決定每個 Node 的輪詢週期；
Modbus 的逾時與重試次數寫在設定檔，並算進流量預算。

### 1.5 單一 Bridge 的其他限制

| 問題 | 對策 |
|---|---|
| 記憶體：佇列與重組緩衝都是靜態配置，突發流量超過容量就丟 | 建置期依流量預算算出各佇列深度；丟棄有計數並隨 Heartbeat 回報 |
| 跨 Bridge 的 Service 呼叫要記住進行中的 request | 進行中表格有上限，逾時清除；上限寫進規格 |
| 單點故障 | Bridge 確實停止發送（fail-silent）時，同匯流排內的通訊不受影響；Bridge 的錯誤若讓它在 CAN 上洪泛，同匯流排也會被拖累（K-26）；跨匯流排的需求可以拆給多台 Bridge |
| 拆成多台 Bridge 後可能出現轉送迴路，或兩條路徑把同一個（Subject、來源）送進同一條匯流排 | 建置期檢查轉送表沒有迴路，且每個（Subject、來源）進入每條匯流排的路徑唯一（K-27）；延遲報告列出跳數 |

### 1.6 怎麼量

延遲上限先在建置期算：每個跨 Link 的 Subject 依週期、大小、抖動得到 token bucket 到達曲線，出口以 rate-latency 服務曲線描述，
延遲 ≤ T + Σb/R，緩衝 ≤ Σb + ΣrT（network calculus，推導見 `docs/20-protocol/04` 第三節）。
出口佇列依 Priority 排序，所以上限要依優先等級分別計算：較低的等級拿到的是扣掉較高等級後的剩餘服務曲線。量測的作用是驗證這個模型。

P3 加一項負載測試：逐步提高送進 Bridge 的流量，從出口容量的 10% 到 100%，記錄每一級的 p99 與丟棄數，
找出延遲開始急遽上升的轉折點。

通過行：`EMBNET_LOAD in=<link> out=<link> prio=<N> knee_util=<N>% p99_at_50pct_us=<N> max_us=<N> bound_us=<N> drops_at_90pct=<N>`

`max_us` 超過該優先等級的 `bound_us` 時，表示 R 或 T 的模型錯了，要先修模型再看其他數字。
上限對給定的曲線是緊的，但實際的到達與服務都比曲線好，所以量到的值通常會低於上限。

## 二、技術可行性

| 編號 | 風險 | 成因 | 最早看得出來 | 對策 |
|---|---|---|---|---|
| K-01 | classic CAN 上加 MAC 後，單框可用資料只剩 2 到 3 B | 8 B 中要放 Tail byte、序號與截短 MAC | P1 的 CAN 配置計算 | `protected` Subject 走 CAN FD，或以 Transfer 為單位放一個 MAC；參考板選有 FDCAN 的晶片 |
| K-02 | 7 位元 Node-ID 不夠用 | 全系統共用一個 Node-ID 空間 | P1 估算系統規模 | 預留 CAN ID 的配置彈性；不在 Bridge 轉換 Node-ID（會讓 E2E 與 MAC 失效），改為擴充來源 ID 的寬度，例如由 E2E 邏輯標頭帶完整的來源 ID |
| K-03 | 期限檢查無法限制資料的年齡 | 單向的週期發佈只看接收間隔，只偵測得到「停止更新」；Bridge 把每筆都多壓一段固定時間再照週期送出，接收間隔看起來完全正常 | P1 寫規格時 | 本地計時只用來偵測停止更新；要限制年齡的 `protected` Subject，採一問一答式的序號回送（PROFIsafe 的做法），或加對時（原理見 `docs/30-safety-security/01` 第四節） |
| K-04 | 截短 FV 失步，合法訊息被拒 | 線上只送 FV 的低位（由 E2E 序號兼任），連續漏收超過低位的範圍，高位就重建錯誤 | P6 的 FV 同步測試 | 定期送完整 FV；失步期間的行為（拒收或降級）由 Subject 等級決定 |
| K-05 | 軟體 AES 太慢 | STM32F427 沒有硬體 AES（已確認），只能用軟體；常數時間的實作可能吃掉整個每框預算 | P1 實測週期數 | 見 1.2；只在 Trust zone 邊界驗證；需要時改選有硬體 AES 的晶片（例如 F437、F439） |
| K-06 | 不同格式的 Link 之間轉送時，語意不清 | CAN FD 的 64 B 轉成 classic 的 8 B 要重新分片；CAN 與 UDP 的 Transfer-ID 寬度與 CRC 都不同 | P1 寫規格時 | 規格明定兩層：E2E 保護與 MAC 只綁與 Link 無關的邏輯標頭（Subject-ID、來源 Node-ID、E2E 序號、payload），以 Transfer 為單位，端到端保留；Transfer-ID、Tail byte、多框 CRC 屬於 Link 層，由 Bridge 重做 |
| K-07 | Polled link 的 Native node 要多寫 Mailbox 韌體 | SPI、I²C 從端沒有主動發話的能力 | P4 | Mailbox 格式寫成規格，`embnet-node` 提供從端實作 |
| K-08 | I²C 交易時間沒有上限 | 從端可以用 clock stretching 讓主端無限期等待 | P4 | Polled link 規格限制或禁止 clock stretching；Bridge 對每筆交易設逾時（原理見 `docs/10-buses/02` 第 3.3 節） |
| K-09 | RS-485 的最後一個 byte 被截斷，或方向切換太慢 | 線路驅動器在 stop bit 送完前就被關掉；或軟體切換延遲沒算進預算 | P3 | 用 transmission complete 旗標控制方向；切換時間實測後算進流量預算 |
| K-27 | 同一條 CAN 上出現兩個發送者送同一個 CAN ID | Node-ID 全系統共用，Bridge 轉送時保留來源；兩條路徑把同一個（Subject、來源）送進同一條匯流排時，兩邊同時發送會造成 bit error、錯誤計數上升，最後 bus off | P1 寫轉送表規則時 | 建置期檢查每個（Subject、來源）進入每條匯流排的路徑唯一；規格寫明 Bridge 的發送緩衝配置 |
| K-29 | 同一筆多框 Transfer 的框被控制器重排 | 依序到達要靠發送端：bxCAN 依 ID 決定送出順序，ID 相同時依 mailbox 編號，同一 session 的框若放進不同 mailbox 就可能亂序；把 MAC 位元放進 CAN ID 會讓每框 ID 不同，重排更嚴重 | P1 | 不把 MAC 放進 CAN ID；規格寫明同一 session 同時只占一個發送 mailbox，或用 FIFO 模式並說明它對優先序的影響（原理見 `docs/20-protocol/02` 第二節） |

## 三、規格與驗證

| 編號 | 風險 | 成因 | 最早看得出來 | 對策 |
|---|---|---|---|---|
| K-10 | 規格和實作一起錯，測試卻全綠 | 不與 Cyphal 相容，沒有外部對照組 | P1 之後的每個里程碑 | test vector 至少一份由獨立的 Python 程式產生；規格先於實作合入 |
| K-11 | 借用的 Cyphal 設計日後改變 | Cyphal v1.1 目前只有論壇上的早期預覽，作者自述還不是正式的 RFC | P0 | 不相容就不受影響；借用時記下對應的 Cyphal 版本與章節 |
| K-12 | 想沿用的既有程式碼授權不相容 | canadensis 等專案的授權尚未逐一確認 | P0 | 沿用任何程式碼前先記下授權；與 MIT 不相容就只參考設計 |
| K-13 | 跨 Link 轉送時 Transfer-ID 對不上 | Cyphal/CAN 的 Transfer-ID 是 5 位元循環，UDP 是 64 位元；Cyphal 社群的結論是兩者沒有直接對應、Service 無法跨網段 | P1 寫規格時 | Transfer-ID 定位為 Link 層的欄位，Bridge 為每個跨 Link 的 session 維護對應表與進行中的 request；端到端的順序與去重複改由 E2E 序號負責（K-06；見 `docs/20-protocol/01` 第十一節） |

## 四、Safety 與 Security

| 編號 | 風險 | 成因 | 最早看得出來 | 對策 |
|---|---|---|---|---|
| K-14 | 重組 slot 被耗盡 | 攻擊者或故障的 Node 不斷送「永遠缺一片」的多框 Transfer，占住 Bridge 或接收端的重組緩衝（RFC 8900 第 3.7 節） | P2 的故障注入 | 每個來源的 slot 數有上限；殘框用短逾時清除；slot 用完的次數計入 security 事件（原理見 `docs/20-protocol/02` 第五節） |
| K-15 | E2E CRC 選得太弱 | CRC-15/CAN 帳面上在 8 B 框內有 HD 6，但填充位元出錯會讓位元錯位，實際只剩 HD 2（FAA DOT/FAA/TC-14/49 第 5.2 節）；black channel 下錯誤率高時，漏抓機率由 2⁻ʳ 主導，CRC-8 每筆錯誤約有 4 × 10⁻³ 的機率漏抓 | P1 | `protected` Subject 一律加 E2E CRC；殘餘錯誤率算出來之前，不用 CRC-8；P1 依殘餘錯誤率與「單位時間錯誤數超限就進入安全狀態」的監控一起選 CRC（`docs/20-protocol/03` 第十一節） |
| K-16 | 暫存的舊訊息被重送後通過序號檢查 | 線上的序號很短時，舊訊息可能帶著「下一個」序號被接收；black channel 下不能假設中間元件只暫存建置期算出的量（PROFIsafe 因此用 32 位元序號） | P1 | E2E 序號與 Transfer-ID 分開；比照 FV，用完整寬度的隱含序號算進 E2E CRC，線上只送低位；有沒有 MAC 都適用（原理見 `docs/30-safety-security/01` 第 2.1 節） |
| K-17 | 重開機後 FV 回到舊值，舊訊息可被重放 | 完整 FV 存在 RAM，重開機後從 0 開始 | P6 | 完整 FV 的高位寫入非揮發記憶體（SecOC 附錄 A.4 的 TripCnt 做法），或每次開機衍生新的 session 金鑰（LeiA 的 epoch 做法）；注意快閃記憶體的寫入壽命 |
| K-18 | 一個接收端被攻破，就能偽造同一金鑰下的所有發送端 | 對稱金鑰在一對多廣播時，接收端也持有發送用的金鑰（RFC 4082 第 1 節） | P1 | 依發送端分金鑰，縮小影響範圍；跨 Trust zone 只由 Bridge 驗證再轉入；TESLA 類的延遲揭露不適合即時控制（vatiCAN 第 7 節），不採用 |
| K-19 | 截短 MAC 的失敗預算被用完 | 24 位元 MAC 每把金鑰只容許約 16 次驗證失敗（NIST SP 800-38B 附錄 A.2，風險取 2⁻²⁰） | P1 | 優先用 CAN FD 的 64 位元 MAC；classic CAN 至少 32 位元；失敗預算的處理見 K-28 |
| K-20 | 使用者把 Bridge 當成安全功能 | 統一介面讓人想把所有訊號都接上去 | 第一個應用整合時 | 文件與 README 寫明限制；`protected` Subject 的接收端一定要有本地的安全狀態 |
| K-21 | 部署時沒有安全的金鑰儲存 | 金鑰管理屬於平台責任，本專案不涵蓋 | 第一次實板部署 | 定義金鑰介面；沒有安全儲存時，在 Heartbeat 標示認證功能未啟用 |
| K-22 | Bridge 成為攻擊跳板 | 原本隔離的乙太網路與 CAN 被接通 | P1 的轉送表設計 | 跨 Trust zone 預設拒絕；P6 測試未列入的轉送全部被拒 |
| K-23 | CAN 的 bus-off 攻擊無法用軟體擋 | 攻擊者與受害者同時送同一個 ID，讓受害者反覆發生 bit error，錯誤計數超限而離線（Cho & Shin 2016）；攻擊發生在資料驗證之前，MAC 防不了 | P6 | 只能偵測：TEC、REC 與 bus off 狀態納入 Heartbeat；控制器與收發器層的防護不在本專案範圍，文件寫明（原理見 `docs/10-buses/01` 第 6.3 節） |
| K-24 | 解析器漏洞 | Bridge 直接面對對外 Link 的輸入 | P2 起的 fuzz | 核心 `forbid(unsafe_code)`；fuzz 覆蓋率列入通過行 |
| K-25 | Bridge 自己被重刷或改設定，成為跳板 | Jeep Cherokee 案例：橋接晶片的韌體可由上游更新且沒有簽章，轉送不過濾（Miller & Valasek 2015） | P1 | 韌體與轉送表不能從對外 zone 更新；更新須經平台的簽章驗證，來源限定為攻擊面最小的介面；執行期只能切換預先定義的轉送表組（原理見 `docs/30-safety-security/03`） |
| K-26 | Bridge 出錯時沒有真的停止發送 | 堆疊溢位在 Cortex-M 預設配置下靜默覆寫全域變數；flip-link 擋下的溢位會進 HardFault 而不是 panic handler；例外處理時壓堆疊失敗可能讓核心 lockup；CPU 停住後 CAN 控制器仍會把 mailbox 中的框送完並自動重送；`no-panic` 的證明在 thumb 的預設 abort 建置下無效 | P5 | 規格涵蓋 panic、HardFault、lockup、看門狗四條路徑；必要時用 GPIO 控制 CAN 收發器的 standby 腳，從硬體切斷輸出；韌體用 `flip-link`，`paint-stack` 量高水位；解析函式在主機端 unwind 建置以 `#[no_panic]` 驗證；P5 做故障注入驗證（原理見 `docs/40-rust-embedded/01`） |
| K-28 | MAC 失敗預算被拿來做阻斷服務 | 攻擊者不必偽造成功，重放舊框或亂送 MAC 都算一次驗證失敗；預算用完就判 INVALID，`protected` Subject 停擺 | P1 | 威脅表把它列為阻斷服務；失敗計數依來源、Link 與時間窗分開；明確定義「判 INVALID」與「要求換金鑰」各自的觸發條件與恢復路徑；換金鑰流程與平台一起定義 |

## 五、工具與環境

| 編號 | 風險 | 成因 | 最早看得出來 | 對策 |
|---|---|---|---|---|
| K-30 | 主機要載入 `vcan` 模組、容器要 `CAP_NET_ADMIN` | SocketCAN 在核心裡 | P0 | 先取得同意；只在測試容器開這項權限 |
| K-31 | 模擬器不支援 bxCAN 或 FDCAN | Renode 對 STM32 CAN 周邊的支援尚未確認 | P0 | P0 先確認；不支援就提早準備實板 |
| K-32 | 主機上的延遲數字被誤當成真實延遲 | `vcan` 與 pty 沒有 Wire time | 每次報告 | 通過行標明環境；主機數字只稱為處理時間 |
| K-33 | 沒有實板或邏輯分析儀 | 硬體尚未決定 | P3 | `001` 的待審查決定先處理參考板 |

## 六、範圍與進度

| 編號 | 風險 | 成因 | 最早看得出來 | 對策 |
|---|---|---|---|---|
| K-40 | 範圍太大，每一項都做一半 | 四類介面、兩種平台、safety 與 security 同時進行 | P2 完成時間 | 依 CAN、UDP 與 UART、Polled link 的順序推進，每步都要有通過行才前進 |
| K-41 | 知識庫與實作脫節 | 知識庫寫在前，實作後來改了 | P1 之後 | 規格以 `docs/spec/` 為準；知識庫文章引用規格章節，不另寫一份 |
