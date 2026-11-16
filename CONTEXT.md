# 術語表

格式：`術語`：定義。_避免_：不要混用的同義詞。

- `Router`：在多條 Link 之間轉送訊息的程式，保管 Routing table、session 與 topic 註冊。對應 XRCE 的 agent 角色。_避免_：agent、gateway、bridge。
- `Node`：接在某條 Link 上、會收發訊息的裝置。分成 Native client 與 Legacy device。_避免_：endpoint（另有意義）、station。
- `Native client`：跑 embnet client 程式庫、直接講 embnet 線格式的 Node。
- `Legacy device`：只講原生協定（例如 Modbus RTU、自訂 CAN 訊號）的 Node，由 Router 上的 Adapter 代為翻譯。
- `Adapter`：Router 裡把 Legacy device 的原生協定對應到 topic 的元件，例如 Modbus 暫存器對應 topic 欄位。
- `Link`：Router 的一個實體介面加上它的 framing，例如 `uart0`（HDLC）、`can1`（CAN FD）。_避免_：port、bus（指實體匯流排時才用）。
- `Topic`：一種具名、有固定型別的資料流。
- `Topic ID`：Topic 在線上的數字代號，在建置時由設定產生。
- `Service`：一問一答的呼叫，由一對 request 與 reply 訊息組成。
- `Endpoint`：某個 Node 上對某個 Topic 或 Service 的發佈端或訂閱端。
- `Route`：一條「從哪條 Link 的哪個 Endpoint 送到哪些 Link」的轉送規則。
- `Routing table`：所有 Route 的集合。預設在建置時固定，執行中不新增。
- `Hop`：訊息經過一次 Router。單跳延遲從入口 Link 收到最後一個位元組算到出口 Link 送出最後一個位元組。
- `Wire time`：訊息在某條 Link 上序列化所需的時間，由速率與框架長度決定，是延遲的物理下限。
- `Router 處理時間`：單跳延遲扣掉兩端 Wire time、排隊與輪詢等待後的部分。
- `E2E 標頭`：由發送端 Node 加上、接收端 Node 檢查的保護欄位（序號、Data ID、CRC，必要時加 MAC），Router 不改寫。
- `Trust zone`：信任等級相同的一組 Link，例如板內匯流排與對外乙太網路分屬不同 Trust zone。

## 待釐清

- 日後若要接 MQTT、HTTP 這類對外協定，那個元件叫 Adapter 還是另取名字，等需求出現再定。
