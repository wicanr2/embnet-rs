# 007 P2 結果與實板初測

狀態：P2 的軟體部分完成。實板只有一塊 NUCLEO-F767ZI，沒有 CAN 收發器，兩個 Node 之間的真實匯流排測試、`vcan` 與 SocketCAN 的 Link 延後。

## 一、通過行

```
EMBNET_FAULT PASS kind=corrupt seed=63526376483197206 transfers=10000 injected=484 detected=484 wrong_delivered=0 frames_affected=484 env=sim run=runs/20261116-191532-p2-bus
EMBNET_FAULT PASS kind=delay seed=63526376483197206 transfers=10000 injected=36 detected=36 wrong_delivered=0 frames_affected=36 env=sim run=runs/20261116-191532-p2-bus
EMBNET_FAULT PASS kind=duplicate seed=63526376483197206 transfers=10000 injected=481 detected=481 wrong_delivered=0 frames_affected=481 env=sim run=runs/20261116-191532-p2-bus
EMBNET_FAULT PASS kind=loss seed=63526376483197206 transfers=10000 injected=460 detected=460 wrong_delivered=0 frames_affected=481 env=sim run=runs/20261116-191532-p2-bus
EMBNET_FAULT PASS kind=reorder seed=63526376483197206 transfers=10000 injected=459 detected=459 wrong_delivered=0 frames_affected=459 env=sim run=runs/20261116-191532-p2-bus
EMBNET_BUS PASS link=can-sim env=host-sim case=normal_2B_single_frame samples=100000 p50_us=0.15 p99_us=0.26 p99_ci_us=0.26-0.27 max_us=24.29 run=runs/20261116-191532-p2-bus
EMBNET_BUS PASS link=can-sim env=host-sim case=protected_2B_single_frame samples=100000 p50_us=0.28 p99_us=0.63 p99_ci_us=0.61-0.64 max_us=30.22 run=runs/20261116-191532-p2-bus
EMBNET_BUS PASS link=can-sim env=host-sim case=normal_20B_multi_frame samples=100000 p50_us=0.55 p99_us=0.92 p99_ci_us=0.91-0.93 max_us=29.34 run=runs/20261116-191532-p2-bus
EMBNET_FUZZ PASS target=node_on_frame hours=0.083 corpus=1243 cov_metric=line cov=91.94% cov_scope=embnet-node+can/+e2e.rs+heartbeat.rs cov_crate=66.50% crashes=0 run=runs/20261116-185449-p2-fuzz
EMBNET_FUZZ PASS target=datagram hours=0.083 corpus=735 cov_metric=line cov=46.47% cov_scope=datagram.rs+crc.rs cov_crate=11.16% crashes=0 run=runs/20261116-185449-p2-fuzz
EMBNET_FUZZ PASS target=e2e_check hours=0.083 corpus=48 cov_metric=line cov=53.90% cov_scope=e2e.rs cov_crate=13.42% crashes=0 run=runs/20261116-185449-p2-fuzz
EMBNET_FUZZ PASS target=config_parse hours=0.083 corpus=6499 cov_metric=line cov=87.80% cov_scope=embnet-config cov_crate=34.58% crashes=0 run=runs/20261116-185449-p2-fuzz
EMBNET_P0_SIZE impl=embnet-node target=thumbv7em-none-eabihf cpu=cortex-m7 text=2932 rodata=1760 data=0 bss=656 heap=0 profile=z,lto run=runs/20261116-185531-p0-size
EMBNET_P0_SIZE impl=embnet-node-full target=thumbv7em-none-eabihf cpu=cortex-m7 text=8368 rodata=2088 data=0 bss=2392 heap=0 profile=z,lto run=runs/20261116-185531-p0-size
EMBNET_BOARD_COST board=nucleo-f767zi sysclk_hz=216000000 cache=on art=on unit=cycles k=200 mac32_gen=11024 mac32_verify=11379 e2e32_gen=132 e2e32_verify=140 canid_decode=8 segment_9B=57 reassemble_2f=350 datagram_decode_20B=223 mac32_gen_cached=8244 mac32_verify_cached=8518 mac_key_expand=2839 run=runs/20261116-192158-p2-board
EMBNET_BOARD_CAN_ORDER PASS board=nucleo-f767zi can=CAN1 mode=loopback-silent prefill_txfp0=0x100,0x200,0x300 prefill_txfp1=0x300,0x100,0x200 live_txfp0=0x300,0x100,0x200 live_txfp1=0x300,0x200,0x100 put_fail=0 run=runs/20261116-192158-p2-board
```

## 二、`embnet-node`

- sans-IO 的 Native node（CAN）：發佈、訂閱、Service 的 request 與 response、Heartbeat、E2E-32 與 Sync、接收狀態機與期限。`no_std`、`forbid(unsafe_code)`，容量全部是 const generic。
- 發送佇列依 CAN ID 由小到大、同 ID 依進入順序取出，同一個 session 的框不會在 Node 內被重排（`003` K-29）。
- 體積（與 P0 同一個外殼相減，`opt-level = "z"`）：只發佈的路徑 4,692 B 程式、656 B 靜態 RAM；含接收、E2E、Sync、Service 的完整路徑 10,456 B 程式、2,392 B 靜態 RAM。作為對照，P0 量到的 libcanard 只發佈路徑是 6,189 B（`005` 第二節）。

## 三、故障注入

模擬匯流排上，`protected` 指令以 10 ms 週期送 10,000 筆，每種故障以 5% 的機率注入（延遲為 0.5%，每次停止送達 200 ms，期限 50 ms）。seed 固定，失敗時可以重現。

| 故障 | 注入的定義 | 偵測的定義 | 結果 |
|---|---|---|---|
| 遺失 | 缺口數（連續遺失算一個缺口）；481 框形成 460 個缺口 | 接收端判為 OKSOMELOST 或 WRONGSEQUENCE 的次數 | 460 / 460 |
| 重複 | 多送一次的框數 | 傳輸層去重複的次數 | 481 / 481 |
| 亂序 | 晚到的框數 | 晚到的框被判為 ERROR、REPEATED 或 WRONGSEQUENCE 的次數 | 459 / 459 |
| 毀損 | 資料或 E2E 欄位翻一個位元的框數（不含 Tail byte） | E2E ERROR、toggle、重組與 CRC 錯誤的總和 | 484 / 484 |
| 延遲 | 停止送達的次數 | 期間至少產生一次 NONEWDATA 的次數 | 36 / 36 |

五種都沒有把錯誤的資料交給應用（`wrong_delivered=0`）。毀損只改資料與 E2E 欄位：在真實的 CAN 上，線上的毀損由 CAN 自己的 CRC 擋下，這裡模擬的是 Bridge 記憶體或沒有 CRC 的 Link（SPI、I²C）上的毀損。

## 四、處理時間與 fuzz

- 處理時間是主機上「publish 到接收端交出事件」的軟體時間，沒有 Wire time（`003` K-32）。各 100,000 筆，p99 在 0.26 到 0.92 µs。
- fuzz 每個 target 5 分鐘，0 個 crash。覆蓋率以該 target 涵蓋的檔案計，`cov_crate` 是整個 crate 的數字。`datagram` 與 `e2e_check` 偏低，是因為範圍內的檔案有該 target 不會呼叫的函式（例如 `crc.rs` 的其他 CRC、`e2e.rs` 的狀態機）。

## 五、實板初測

板子：NUCLEO-F767ZI，216 MHz（由 HSI 經 PLL），I-cache、D-cache 與快閃記憶體的 ART 加速開啟。韌體在 `probes/p2-board`，`tools/p2-board.sh` 燒錄後用 probe-rs 讀回 RAM 裡的結果。

### 5.1 每一步的週期數

| 步驟 | 實機週期數 | 216 MHz 下的時間 | Renode 推算的指令數（`006`） |
|---|---|---|---|
| CAN ID 解碼 | 8 | 0.04 µs | 6 |
| 分框：9 B 切成 2 框 | 57 | 0.26 µs | 64 |
| E2E-32 產生 | 132 | 0.61 µs | 102 |
| E2E-32 驗證 | 140 | 0.65 µs | 115 |
| datagram 解碼與 CRC-32C（20 B） | 223 | 1.03 µs | 218 |
| 重組：2 框 | 350 | 1.62 µs | 372 |
| `mac32` 驗證，金鑰預先展開 | 8,518 | 39.4 µs | 13,321 |
| `mac32` 產生，金鑰預先展開 | 8,244 | 38.2 µs | 13,224 |
| 金鑰展開 | 2,839 | 13.1 µs | 4,729 |

- 除了 MAC，一框的協定成本約 500 個週期，遠低於 `003` 第 1.2 節兩條滿載 CAN 時每框約 4,050 週期的預算。
- `mac32` 驗證一則約 8,500 個週期，是每框預算的兩倍。留一半 CPU 時，每秒約可驗 12,700 則。兩條滿載 CAN 約 26,667 框/s，不可能逐框驗證。`003` K-05 的對策不變：只驗跨 Trust zone 的 `protected` Subject，並以建置期檢查限制每秒的驗證數（`006` 第四節建議的 C10），係數取 8,518 週期。
- AES 的部分比 Renode 推算的指令數少約 36%（Cortex-M7 的雙發射），其他步驟兩者接近。Renode 的指令數可以當上限的粗估，絕對值以實機為準。

### 5.2 bxCAN 的送出順序

CAN1 用內部 loopback 加 silent 模式，不需要收發器。

| 情況 | 送入 mailbox 的順序 | 收到的順序 |
|---|---|---|
| TXFP=0，在初始化模式放好三框再啟用 | 0x300、0x100、0x200 | 0x100、0x200、0x300 |
| TXFP=1，同上 | 0x300、0x100、0x200 | 0x300、0x100、0x200 |
| TXFP=0，啟用後依序寫入 | 0x300、0x200、0x100 | 0x300、0x100、0x200 |
| TXFP=1，同上 | 0x300、0x200、0x100 | 0x300、0x200、0x100 |

- TXFP=0 時，同時待送的 mailbox 依 ID 決定先送哪一個；已經開始送的那一框不受影響。TXFP=1 時依請求順序。這與 Renode 的模型不同（模型一律依請求順序，`005` 第四節）。
- 對 K-29 的意義：Link 驅動用 TXFP=0 保留優先序時，同一個 session 的下一框必須等前一框離開 mailbox 才能放入，否則同 ID 的框會依 mailbox 編號送出。這條規則寫進 P3 的 `embnet-link-can`。
- 測試中觀察到：同一支韌體第三次建立 `Can` 之後，embassy-stm32 0.6.0 的 async `write().await` 沒有返回；改用 `try_write` 就正常。原因未查，P3 寫驅動時再確認。

## 六、還沒做的

| 項目 | 原因 | 何時 |
|---|---|---|
| `vcan` 直連的處理時間、SocketCAN 的 Link | 使用者決定延後 | `vcan` 載入後 |
| 兩個 Node 之間的真實 CAN、bus-off、Wire time | 只有一塊板子，沒有收發器 | 第二塊板子與收發器到位後（`004` 第八節） |
| UDP | 板子的乙太網路沒有接線 | 接線後 |
| C10：每秒 MAC 驗證數的建置期檢查 | 需要實機週期數，現在已有 | P3 |
