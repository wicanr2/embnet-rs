# 術語表

格式：`術語`：定義。_避免_：不要混用的同義詞。多數術語沿用 Cyphal 的用法，方便對照它的規格。

## 網路結構

- `Node`：接在某條 Link 上、會收發 Transfer 的裝置。分成 Native node 與 Legacy device。_避免_：station、endpoint。
- `Native node`：跑 embnet 程式庫、直接講 embnet 線格式的 Node。
- `Legacy device`：只講原生協定（例如 Modbus RTU）的 Node，由 Bridge 上的 Adapter 代為翻譯。
- `Node-ID`：Node 在整個系統中的數字代號，設計期指定，全系統共用一個空間。
- `Link`：一條實體介面加上它的 framing，例如 `can1`（bxCAN）、`uart0`（UART 加上 COBS framing）。_避免_：port。
- `MTU`：一個 Frame 最多能帶的資料量，例如 classic CAN 8 B、CAN FD 64 B。
- `轉送表`：`embnet-config` 在建置期產生的規則表，列出哪些（Subject、來源）可以從哪條 Link 轉往哪條 Link。執行期不能新增規則。_避免_：routing table（只在引用外文時用）。
- `主端`、`從端`：Polled link 上發起交易的一方與回應的一方。Modbus 文件稱為主站、從站，指的是同一件事。
- `Polled link`：只有主端能發起交易的 Link，例如 SPI、I²C、Modbus RTU。Node 的輸出要等主端來讀。
- `Mailbox`：Native node 在 Polled link 上放待送 Transfer 的緩衝區，由 Bridge 輪詢讀取。
- `DRDY`：Polled link 上 Native node 拉給 Bridge 的中斷線，位準高表示 Mailbox 不是空的。_避免_：IRQ、INT（指其他晶片的腳位時才用）。
- `輪詢時刻表`：`embnet-config` 在建置期產生的 cyclic executive 時刻表，決定 Bridge 在每個 minor cycle 輪詢哪些 Node。
- `Bridge`：在不同 Link 或不同 Trust zone 之間轉送 Transfer 的節點。同一條匯流排上的 Node 直接互通，不經過 Bridge。_避免_：router、agent、gateway。
- `Adapter`：Bridge 裡把 Legacy device 的原生協定對應到 Subject 的元件。
- `Trust zone`：信任等級相同的一組 Link。Bridge 是 Trust zone 的邊界。

## 資料模型

- `Subject`：一種有固定型別的發佈資料流，相當於 MQTT 或 ROS 2 的 topic。_避免_：topic、channel。
- `normal`、`protected`：Subject 的兩個等級，在設定檔標註。`protected` 指錯了會造成設備或人員風險的資料，例如運動指令，強制 E2E 保護並由接收端判斷能不能用（`docs/plans/002` 第一節）。
- `Subject-ID`：Subject 在線上的數字代號，設計期指定。
- `Service`：一問一答的呼叫，由 request 與 response 兩個 Transfer 組成。
- `Service-ID`：Service 在線上的數字代號。
- `Type hash`：型別定義的雜湊，用來在建置期與執行期確認收發兩端的型別一致。
- `Transfer`：一次完整的發佈、request 或 response。可能拆成多個 Frame。_避免_：message（只在指型別時用）、packet。
- `Frame`：一個 Link 層的框，例如一個 CAN 框。
- `session`：同一個（Subject 或 Service、來源 Node-ID）的 Transfer 序列。Transfer-ID 在每個 session 內各自遞增，重組狀態也以 session 為單位保存。
- `Transfer-ID`：每個 session 各自遞增的序號，屬於 Link 層，用來重組與在單一 Link 上去重複。跨格式轉送時由 Bridge 重做。
- `slot`：接收端為一個進行中的多框 Transfer 保留的重組緩衝。數量固定，用完就無法再開始新的重組。
- `extent`：接收端為某個型別預留的最大長度。比 extent 長的資料被截掉，較短的補 0，讓型別可以向後相容地加欄位。
- `sealed`：宣告這個型別不再演進，長度固定，不需要 extent 的預留空間。
- `Tail byte`：每個 Frame 最後一個位元組，放 start、end、toggle 與 Transfer-ID 的低位。沿用 Cyphal/CAN 的設計。
- `Priority`：Transfer 的優先等級，在 CAN 上直接對應仲裁順序。
- `Heartbeat`：每個 Node 定期發佈的存活訊息，帶健康狀態與運作模式。

## 保護

- `功能安全`、`資安`：functional safety 與 security。前者處理系統自己出錯（雜訊、故障），後者處理有人故意讓它出錯。本 repo 單寫「安全」時，盡量改用這兩個詞之一。
- `E2E 保護`：發送端 Node 加上、接收端 Node 檢查的保護。只綁與 Link 無關的邏輯標頭（Subject-ID、來源 Node-ID、E2E 序號）與 payload，以 Transfer 為單位，Bridge 轉送時不改寫。沒有認證時用 CRC，有認證時用 MAC。
- `E2E 序號`：E2E 保護裡的序號，與 Transfer-ID 分開。發送端維護完整寬度、算進 CRC 或 MAC，線上只送低位。
- `MAC`：以金鑰計算的訊息認證碼。可截短，長度決定偽造成功的機率。_避免_：指紋、簽章（簽章指非對稱演算法）。
- `FV`：Freshness value，單調遞增的新鮮度值，加進 MAC 計算以防重放。線上只送低位（截短 FV）。

## CAN

- `dominant`、`recessive`：CAN 線上的兩種狀態，分別是邏輯 0 與 1。任一 Node 送 dominant，整條線就是 dominant。_避免_：顯性、隱性（只在首次解釋時附上）。
- `仲裁`（arbitration）：多個 Node 同時發送時，逐位元比較 ID，ID 小的勝出，勝出者的框不受破壞。
- `填充位元`（stuff bit）：每 5 個相同位元後插入的反相位元，讓接收端有邊緣可以校正時鐘。
- `TEC`、`REC`：發送與接收錯誤計數器，決定 Node 處於 error active、error passive 或 bus off。
- `bus off`：TEC 達 256 後，Node 不得再影響匯流排的狀態。
- `標準框`、`延伸框`：ID 為 11 位元與 29 位元的 CAN 框，由 IDE 位元區分。embnet-rs 用延伸框。

## MCU

- `MCU`：微控制器，CPU、快閃記憶體、RAM 與周邊做在同一顆晶片上。程式放在快閃記憶體，變數放在 RAM。
- `HardFault`：Cortex-M 遇到非法存取等嚴重錯誤時進入的例外處理，類似一般作業系統的 segfault。
- `DMA`：周邊直接搬資料進出 RAM，不經過 CPU。

## 量測

- `Hop`：Transfer 經過一次 Bridge。
- `Wire time`：Frame 在 Link 上序列化所需的時間，由速率與框長決定，是延遲的物理下限。
- `Bridge 處理時間`：單跳延遲扣掉兩端 Wire time、排隊與輪詢等待後剩下的部分。

## 待釐清

- 日後若接 MQTT、HTTP 這類對外協定，那個元件叫 Adapter 還是另取名字，等需求出現再定。
