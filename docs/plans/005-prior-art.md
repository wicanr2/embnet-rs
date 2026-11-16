# 005 既有實作對照（P0 結果）

狀態：P0 部分完成。`vcan` 直連的處理時間延後量測（第三節）。方法見 `004-p0-execution.md`，原始輸出在各通過行的 run 目錄（不進版控）。

## 一、結論

| 實作 | 結論 | 理由 |
|---|---|---|
| canadensis 0.6.1 | 只參考設計 | 需要配置器，與核心「不配置記憶體」不合（K-35） |
| emcyphal 0.1.0 | 參考分片、重組與緩衝的寫法，不沿用 | 不配置記憶體，但它是 async API、綁 embassy，與 sans-IO 的核心介面不同；尚不支援 service request |
| libcanard v5.0.0 | C 端對照數字 | 它已經實作 Cyphal v1.1 的 16 位元 Subject-ID；我們只量 v1.0 的 13 位元路徑 |
| cyphal-bridge-and-router | 只參考語意 | Python 概念驗證，Unlicense；轉送語意見第五節 |

所有實作都沒有沿用程式碼，線格式由 P1 的 `docs/spec/` 自訂。授權都相容，日後要沿用個別函式也沒有障礙。

## 二、體積

```
EMBNET_P0_SIZE impl=canadensis target=thumbv7em-none-eabihf cpu=cortex-m7 text=6216 rodata=1820 data=0 bss=608 heap=16384 profile=z,lto run=runs/20261116-173040-p0-size
EMBNET_P0_SIZE impl=emcyphal target=thumbv7em-none-eabihf cpu=cortex-m7 text=7564 rodata=3356 data=0 bss=728 heap=0 profile=z,lto run=runs/20261116-173040-p0-size
EMBNET_P0_SIZE impl=libcanard target=thumbv7em-none-eabihf cpu=cortex-m7 text=5568 rodata=621 data=0 bss=240 heap=4096 profile=z,lto run=runs/20261116-173040-p0-size
```

| 實作 | 程式（text + rodata） | 靜態 RAM（bss） | 另配的 heap 或記憶池 |
|---|---|---|---|
| canadensis | 8,036 B | 608 B | 16 KB（指定值，實際用量未量） |
| emcyphal | 10,920 B | 728 B | 0 |
| libcanard | 6,189 B | 240 B | 4 KB（32 × 128 B 記憶池，指定值） |

量的內容與限制：

- 三者都是「初始化一個 Node，把一筆 5 B 的 `uavcan.primitive.Unstructured` 編成一個 CAN 框，寫進 volatile 位址」，扣掉同一個裸機外殼。只量發送路徑，沒有接收、重組與 service。
- Rust 是 `opt-level = "z"`、`lto = "fat"`；C 是 `-Os` 與 `--gc-sections`，兩者的最佳化等級不完全對等。
- emcyphal 的數字包含 `block_on` 與 embassy-sync 的 mutex，但不含 embassy 執行器；它把 CAN ID 的打包留給驅動，探針用幾行位元運算代替。
- heap 與記憶池是指定的大小。實際峰值用量要在目標上量，P5 再做。
- libcanard 的版本是 v5.0.0（commit `f82291298ef19cf2ae1abee75007a5d8fae83801`）。

對 embnet-rs 的意義：一個只做發送的 Cyphal/CAN 協定層在 Cortex-M7 上約 6 到 11 KB。`embnet-core` 的目標可以設在同一個量級，P2 用同樣的外殼與相減法量。

## 三、`vcan` 直連的處理時間

延後。使用者決定先完成實作，`vcan` 之後再載入。延後的影響：P2 的對照基準要等這一項補上；在那之前，P2 的單一匯流排測試用 sans-IO 的模擬 Link（`docs/40-rust-embedded/02` 第六節）。

順帶確認的事實：canadensis 有 `canadensis_linux` crate 提供 SocketCAN 傳輸（canadensis 的 `basic_node` 範例使用 `LinuxCan` 與 `SystemClock`），補量時不必自寫外層。

## 四、Renode

```
EMBNET_P0_RENODE platform=nucleo_f767zi renode=1.17.0 can_loopback=PASS tx_order_by_id=no rx_order=0x00000300,0x00000200,0x00000100 dwt_hz=72000000 embassy_timer_advances=no run=runs/20261116-173751-p0-renode
```

| 項目 | 結果 | 對 embnet-rs 的影響 |
|---|---|---|
| 平台 | Docker Hub 上的 1.16.1 沒有 `nucleo_f767zi.repl`；1.17.0 有，改用 release 包自建 `embnet-renode:1.17.0-r1` | `docker/renode.Dockerfile` |
| CAN3 loopback | 通過：embassy-stm32 0.6.0 的 bxCAN 驅動在模型上能收發 | 功能測試可以在 Renode 上跑 |
| 送出順序 | 依 ID 0x300、0x200、0x100 連續送出，收到的順序不變，不依 ID 排序 | 與原始碼一致（TXFP 只存不用）。K-29 只能在實板驗 |
| DWT 頻率 | CYCCNT 在 2 秒虛擬時間內增加 144,000,000，等於 72 MHz | 平台描述把 DWT 設為 72 MHz，不是實機的 216 MHz。週期數只在實板量 |
| embassy 的計時 | `Timer::after_millis` 不會返回：預設選到的 TIM12 不在平台描述裡；改用 TIM2 後，TIM2 的計數器仍停在 0 | 需要計時的韌體目前不能在 Renode 上跑。原因待查，P5 前處理 |
| DWT 解鎖 | 模型不處理寫入 Lock Access Register（0xE0000FB0 偏移 0xFB0），只記警告 | 無影響 |

## 五、cyphal-bridge-and-router 的轉送語意

來源：<https://github.com/pavel-kirienko/cyphal-bridge-and-router>，commit `83abbffe5a6298455d2c3af9bcc7fc8e8dbfa6f0`（2022-06-10），Unlicense。它是 Cyphal 論壇〈Cyphal/UDP routing over multiple networks〉討論的示意程式。

它示範兩種做法：

| | Bridge（`bridge.py`） | Router（`router.py`） |
|---|---|---|
| 來源 Node-ID | 保留，以原來源的身分在對面送出（spoof） | 改成 router 自己的 Node-ID |
| Transfer-ID | 用 `TransferIDRectifier` 把循環的 Transfer-ID 展開成單調遞增，對面若是循環的，由傳輸層自動取模 | 每個 Subject 自己從 0 重新編號 |
| Service | 能轉送：展開後的值取模等於原值，所以 response 送回原網段時 Transfer-ID 對得上 | 不轉送 |
| 丟棄條件 | 匿名 Transfer；Node-ID 超出對面的範圍；對面網段已有同一個 Node-ID；目的 Node 不在對面網段 | 無 |
| 輸出逾時 | 用 Transfer-ID 逾時近似，至少一個下限值 | 固定 1 秒 |
| 狀態量 | 每個（port、Node）存 64 位元；CAN 上全開要約 9 MiB（原始碼註解） | 每個 Subject 一個計數器 |

對 embnet-rs 的影響：

- 我們的 Bridge 屬於第一種：保留來源 Node-ID（`003` K-27 的前提）。它丟棄「對面網段已有同一個 Node-ID」的 Transfer，正是 K-27 的執行期版本；我們改在建置期檢查。
- Transfer-ID 的展開表全開要 9 MiB，MCU 放不下。embnet-rs 只為轉送表列出的（Subject、來源）建表，表的大小在建置期決定（`003` K-13）。
- 展開後取模等於原值的性質，讓 Service 可以跨網段。我們的 Service 轉送沿用這個性質，P1 寫進規格。

## 六、授權

```
EMBNET_P0_LICENSE crates=133 non_permissive=0 unknown=0 run=runs/20261116-173815-p0-license
```

`probes/p0-size` 與 `probes/p0-renode` 的 133 個 crate 全部是寬鬆授權（MIT、Apache-2.0、BSD、Unicode-3.0 等組合）。清單在 run 目錄的 `licenses.tsv`。libcanard 是 MIT，cyphal-bridge-and-router 是 Unlicense。
