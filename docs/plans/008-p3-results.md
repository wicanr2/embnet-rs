# 008 P3 結果：Bridge 的模擬驗證與實板 loopback

狀態：P3 的 Bridge 核心完成，在離散事件模擬與實板的 bxCAN loopback 上驗證。兩個 Node 之間的真實匯流排、UDP 與 UART 的實體 Link 延後（只有一塊板子，網路孔沒有接線）。

## 一、通過行

模擬（`tools/p3-bridge.sh`）：

```
EMBNET_HOP PASS in=publisher out=can1 subject=speed_cmd env=sim samples=500 p50_us=367 p99_us=367 max_us=367 bound_us=895 run=runs/20261116-193635-p3-bridge
EMBNET_HOP PASS in=publisher out=eth0 subject=motor_state env=sim samples=250 p50_us=528 p99_us=528 max_us=528 bound_us=1087 run=runs/20261116-193635-p3-bridge
EMBNET_HOP PASS in=publisher out=eth0 subject=imu env=sim samples=500 p50_us=3508 p99_us=3508 max_us=3508 bound_us=3522 run=runs/20261116-193635-p3-bridge
EMBNET_SERVICE PASS via=bridge requests_answered=50 tid_modulo=256->32->256 env=sim run=runs/20261116-193635-p3-bridge
EMBNET_LOAD in=eth0 out=can1 subject=speed_cmd offered_util=10% can1_util=12% samples=690 p99_us=399 max_us=525 bound_us=1014 motor_state_samples=100 motor_state_p99_us=688 motor_state_max_us=688 motor_state_bound_us=1288 rate_limited=0 queue_dropped=0 deadline_dropped=0 env=sim run=runs/20261116-193635-p3-bridge
EMBNET_LOAD in=eth0 out=can1 subject=speed_cmd offered_util=30% can1_util=32% samples=2069 p99_us=432 max_us=524 bound_us=1014 motor_state_samples=100 motor_state_p99_us=688 motor_state_max_us=688 motor_state_bound_us=1592 rate_limited=0 queue_dropped=0 deadline_dropped=0 env=sim run=runs/20261116-193635-p3-bridge
EMBNET_LOAD in=eth0 out=can1 subject=speed_cmd offered_util=50% can1_util=52% samples=3446 p99_us=435 max_us=523 bound_us=1014 motor_state_samples=100 motor_state_p99_us=688 motor_state_max_us=688 motor_state_bound_us=2139 rate_limited=0 queue_dropped=0 deadline_dropped=0 env=sim run=runs/20261116-193635-p3-bridge
EMBNET_LOAD in=eth0 out=can1 subject=speed_cmd offered_util=70% can1_util=72% samples=4828 p99_us=442 max_us=526 bound_us=1014 motor_state_samples=100 motor_state_p99_us=825 motor_state_max_us=848 motor_state_bound_us=3418 rate_limited=0 queue_dropped=0 deadline_dropped=0 env=sim run=runs/20261116-193635-p3-bridge
EMBNET_LOAD in=eth0 out=can1 subject=speed_cmd offered_util=80% can1_util=82% samples=5521 p99_us=453 max_us=549 bound_us=1014 motor_state_samples=100 motor_state_p99_us=848 motor_state_max_us=848 motor_state_bound_us=5032 rate_limited=0 queue_dropped=0 deadline_dropped=0 env=sim run=runs/20261116-193635-p3-bridge
EMBNET_LOAD in=eth0 out=can1 subject=speed_cmd offered_util=90% can1_util=92% samples=6207 p99_us=470 max_us=673 bound_us=1014 motor_state_samples=100 motor_state_p99_us=848 motor_state_max_us=848 motor_state_bound_us=9841 rate_limited=0 queue_dropped=0 deadline_dropped=0 env=sim run=runs/20261116-193635-p3-bridge
EMBNET_LOAD in=eth0 out=can1 subject=speed_cmd offered_util=95% can1_util=97% samples=6554 p99_us=467 max_us=541 bound_us=1013 motor_state_samples=100 motor_state_p99_us=848 motor_state_max_us=981 motor_state_bound_us=19640 rate_limited=0 queue_dropped=0 deadline_dropped=0 env=sim run=runs/20261116-193635-p3-bridge
EMBNET_LOAD in=eth0 out=can1 subject=speed_cmd offered_util=100% can1_util=102% samples=6892 p99_us=476 max_us=529 bound_us=1014 motor_state_samples=100 motor_state_p99_us=935 motor_state_max_us=998 motor_state_bound_us=inf rate_limited=0 queue_dropped=0 deadline_dropped=0 env=sim run=runs/20261116-193635-p3-bridge
EMBNET_ZONE PASS attempts=1 forwarded=0 env=sim run=runs/20261116-193635-p3-bridge
EMBNET_RATELIMIT PASS offered_x10 rate_limited=448 forwarded=137 env=sim run=runs/20261116-193635-p3-bridge
EMBNET_P3_TESTS passed=19 failed_suites=0 run=runs/20261116-193635-p3-bridge
```

實板 loopback（`tools/p3-board.sh`）：

```
EMBNET_HOP PASS in=can1 out=can1 case=single_bus env=board-loopback bitrate=1000000 frames_per_transfer=1 payload_bytes=7 samples=10000 sent=10000 p50_us=160 p99_us=166 p99_ci_us=166-166 max_us=169.9 e2e_bad=0 run=runs/20261116-194328-p3-board
EMBNET_HOP PASS in=can1 out=can3 case=bridge_hop env=board-loopback bitrate=1000000 frames_per_transfer=1 payload_bytes=7 samples=10000 sent=10000 p50_us=319 p99_us=330 p99_ci_us=329-330 max_us=342.4 e2e_bad=0 run=runs/20261116-194328-p3-board
EMBNET_BRIDGE_STATS env=board-loopback forwarded=10021 no_route=0 local=21 queue_dropped=0 rate_limited=0 run=runs/20261116-194328-p3-board
```

## 二、`embnet-bridge`

- sans-IO 的轉送核心，`no_std`、`forbid(unsafe_code)`。入口依序查轉送表（含方向）、限流（token bucket，單位是微秒，每筆消耗一個週期），再放進出口佇列。
- 格式相同的 CAN 到 CAN 逐框直通，Transfer-ID 不變；其他組合先重組，再依出口格式重新分框或包成 datagram。
- Transfer-ID 以 32 位元展開值在模數 32 與 256 之間對應；Service 的 response 從出口回來時，以 request 的展開值反向還原（`docs/spec/06-forwarding.md` 第三節）。
- 每條出口一個佇列，依 CAN ID（datagram 依優先等級）再依進入順序取出；佇列滿時依規則丟最舊的或新來的；逾期的在出隊時丟棄。
- 協定保留的 Subject（8000 以上，例如 Heartbeat）沒有轉送時記在 `local`，不算進 `no_route`，避免正常的本地流量灌高 security 事件。

## 三、模擬驗證

拓撲同 `crates/embnet-config/tests/fixtures/good.toml`：host（UDP，eth0）、bridge1、motor（CAN，can1）、imu（UART，uart0）。CAN 依 ID 仲裁、框長取最壞填充；UDP 與 UART 是全雙工的點對點通道；Bridge 收到後經過設定檔的 `latency_us`（200 µs）才處理。

| 結果 | 說明 |
|---|---|
| 三條路徑的最大延遲都不超過 embnet-config 的上限 | `speed_cmd` 367 µs 對上限 895 µs；`motor_state` 528 對 1,087；`imu` 3,508 對 3,522。UART 這條幾乎是緊的，因為它的延遲幾乎全是 Wire time |
| E2E 原樣通過 Bridge | motor 端 500 筆 `speed_cmd` 全部 OK |
| Service 跨網段 | 50 次 request 都拿到 response，涵蓋 Transfer-ID 在模數 32 上繞圈 |
| 未列入轉送表與限流 | 未列出的 Subject 被丟棄並計數；10 倍速率的洪泛被入口限流擋下 448 筆 |
| 負載轉折點 | `speed_cmd`（優先等級最高）從使用率 12% 到 102%，最大延遲一直在 520 到 670 µs；同一條 CAN 上的 `motor_state`（優先等級 4）從使用率 72% 起開始上升（688 到 848 µs），102% 時到 998 µs。量到的值都低於依優先等級算出的上限 |

模擬過程中找到並修正的設計缺口：**`protected` Subject 跨 Bridge 時，接收端收不到 E2E Sync**，所有結果都是 NOSYNC。現在轉送表會自動附帶同一發佈者、同一路徑的 Sync 規則（`docs/spec/06-forwarding.md` 第五節）。

另外，`embnet-config` 加上 C10：Bridge 每秒要驗證的 MAC 數乘上實測的 8,518 週期，不超過 CPU 預算。

## 四、實板 loopback

板子：NUCLEO-F767ZI，216 MHz。CAN1 與 CAN3 都設為 loopback 加 silent：bxCAN 仍依 1 Mbit/s 的位元時序序列化，所以 Wire time 是真的；silent 模式不驅動 TX 腳，不需要收發器。韌體：`probes/p2-board/src/bin/loop-hop.rs`。

- `protected` Subject（2 B 資料加 5 B E2E 欄位，一個 8 B 的 CAN 框）每 2 ms 一筆，每個階段 10,000 筆。
- 延遲從 Node A 呼叫 `publish` 量到 Node B 交出事件，用 DWT 週期計數器。
- 驅動政策：控制器的發送端空閒時才交下一框，同一時間只有一框在 mailbox 裡（`003` K-29）。

| 路徑 | p50 | p99 | max |
|---|---|---|---|
| Node A ─CAN1→ Node B | 160 µs | 166 µs | 169.9 µs |
| Node A ─CAN1→ Bridge ─CAN3→ Node B | 319 µs | 330 µs | 342.4 µs |

- 一個 8 B 延伸框不含填充是 131 位元（含框間隔），最壞填充 160 位元，所以單一匯流排的 160 µs 幾乎都是 Wire time。
- 經 Bridge 的路徑約等於兩段單一匯流排相加，Bridge 本身的處理時間落在量測的抖動範圍內，與 `007` 第 5.1 節的週期數一致（重組加轉送約 2 µs）。
- 設定檔裡 Bridge 的 `latency_us` 用的是 200 µs，相對實測很保守。P5 量到最壞中斷延遲後再收緊。

## 五、實板上遇到的兩個問題

| 現象 | 原因 | 處理 |
|---|---|---|
| CAN3 停在 `enable()`，MSR.SLAK 一直是 1 | 離開 sleep 與初始化模式要在 CANRX 上看到 11 個連續的 recessive 位元，loopback 模式也一樣。沒有收發器時 RX 腳浮接，CAN3 的 PA8 讀到 0（MSR.RX = 0）；CAN1 的 PD0 剛好浮接為 1 | RX 腳開內部上拉。接上收發器後由收發器驅動，不需要上拉 |
| embassy-stm32 0.6.0 的 `Can::new` 用 `write` 設定 MCR.TTCM = 1（為了接收時戳） | 庫的行為。ES0334 有一條「F76x 的 bxCAN 不支援 time-triggered 模式」的勘誤（條目標題為二手，原文待核對） | 我們不用接收時戳，初始化模式中清掉 TTCM。清掉之後 CAN3 仍停在 sleep，所以它不是上一列的原因 |

`embnet-link-can` 會把這兩點寫進初始化程式。

## 六、還沒做的

| 項目 | 原因 |
|---|---|
| 兩個 Node 之間的真實 CAN、bus-off、仲裁中的真實優先序 | 需要第二塊板子與收發器，或用二極體把兩個控制器的 TX 接成線與 |
| UDP 與 UART 的實體 Link | 網路孔沒接線；UART 可以用 ST-LINK 的虛擬序列埠（`/dev/ttyACM0`），需要讓 `plugdev` 也能讀寫它 |
| `embnet-link-can` 驅動本身 | 初始化、發送政策與上面兩點已在韌體裡驗證，下一步整理成 crate |
| MAC 的跨 zone 驗證 | P6 |
