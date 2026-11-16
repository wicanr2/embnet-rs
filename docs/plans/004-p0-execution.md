# 004 P0 執行細節

狀態：P0 已執行，`vcan` 兩項延後（使用者決定先完成實作）。術語依 `CONTEXT.md`。P0 的目標與完成條件見 `001` 第七節，結果寫進 `005-prior-art.md`。

P0 回答三件事：Cyphal 的既有實作在參考板的目標上占多少空間、在主機的 `vcan` 上直連要多少處理時間、Renode 能模擬到什麼程度。
它也順便把之後每個里程碑都會用到的容器、腳本與通過行格式定下來。

## 一、前置

| 項目 | 狀態 | 做法 |
|---|---|---|
| `vcan` 模組 | 已同意，尚未載入 | 使用者在主機執行 `sudo modprobe vcan`。介面在容器自己的 network namespace 裡建立，不動主機的網路設定 |
| 實板 | P0 不需要 | P3 之前準備，見第七節 |
| 網路 | 只有建映像與抓 crate 時開 | 其餘一律 `--network none` |

在容器裡建 `vcan0` 的做法是給測試容器 `--cap-add NET_ADMIN`，在容器內執行 `ip link add dev vcan0 type vcan`。
`vcan` 能否在非初始的 network namespace 建立，是 P0 的第一個檢查項目；不行就改在主機建立，先回報再做。

## 二、容器映像

映像名稱一律用 `embnet-` 前綴，Dockerfile 放在 `docker/`，基底映像在建置時記下 digest，寫進 `docker/README.md`。

| 映像 | 基底 | 內容 | 用途 |
|---|---|---|---|
| `embnet-rust:1.100.0-r1` | `rust:1.100.0-slim-bookworm` | target `thumbv7em-none-eabihf`；`llvm-tools`、`clippy`、`rustfmt`；`cargo-binutils` 0.4.0、`flip-link` 0.1.12（`cargo install --locked`）；`gcc-arm-none-eabi`（編 libcanard）；`iproute2`、`can-utils` | 建置、體積、`vcan` 量測 |
| `embnet-py:3.13-r1` | `python:3.13.18-slim-trixie` | `pycyphal==1.27.1` | 對照組與日後的 test vector 產生器 |
| `embnet-renode:1.17.0-r1` | `python:3.13.18-slim-trixie`（借用它內含的 libssl） | Renode 1.17.0 linux-portable，release 包以 sha256 核對 | Renode 檢查 |

版本選擇的依據：

- Rust 1.100.0 是 2026-11-12 發布的 stable。
- pycyphal 1.27.1 測過的 Python 是 3.10 到 3.13，所以用 3.13，不用更新的 3.15。
- Renode 最新是 1.17.0（GitHub release），Docker Hub 上最新的編號 tag 是 1.16.1。1.16.1 沒有 `platforms/boards/nucleo_f767zi.repl`，
  所以用 1.17.0 的 release 包自建 `embnet-renode:1.17.0-r1`。
- cargo-fuzz 要 nightly，P2 才用，那時另建映像。probe-rs 要接 USB，P5 才用。

crate 快取放在 docker volume `embnet-cargo`。

## 三、目錄

```
docker/                  Dockerfile 與基底 digest 紀錄
tools/cargo.sh           在 embnet-rust 容器裡跑 cargo
tools/p0-size.sh         體積量測
tools/p0-vcan.sh         vcan 直連的處理時間（延後）
tools/p0-renode.sh       Renode 檢查
tools/p0-license.sh      相依授權清單
probes/p0-size/          Rust 的最小程式：baseline、canadensis、emcyphal
probes/p0-size-c/        C 的最小程式：baseline、libcanard
probes/p0-vcan/          兩個 Node 互發的量測程式
runs/<時間>-p0-<項目>/    每次執行的原始輸出，不進版控
```

`runs/` 列進 `.gitignore`。通過行與摘要抄進 `005-prior-art.md`，並註明 run 目錄名稱。

## 四、體積

### 4.1 量法

用相減：`baseline` 是同樣的裸機外殼（向量表、reset、panic handler、把輸出寫進一個 volatile 位址），但不碰協定；
各實作的程式在同一個外殼裡做「初始化一個 Node，把一筆 8 B 的 Subject 編成 CAN 框，寫進 volatile 位址」。兩者相減才是協定本身的成本。

| 項目 | 設定 |
|---|---|
| target | `thumbv7em-none-eabihf`，`-C target-cpu=cortex-m7` |
| profile | `opt-level = "z"`、`lto = "fat"`、`codegen-units = 1`、`panic = "abort"` |
| 量的區段 | `.text`、`.rodata`、`.data`、`.bss`（`cargo size -- -A`） |
| 不接驅動 | 只量協定層；bxCAN 驅動屬於 Link crate，P5 再量 |

### 4.2 三個實作

| 實作 | 版本 | 授權 | 注意 |
|---|---|---|---|
| canadensis | 0.6.1 | MIT OR Apache-2.0 | `no_std` 但需要 `alloc`（K-35）。用 `embedded-alloc` 當全域配置器，heap 大小固定為 16 KB 並另列；`embedded-alloc` 的版本與授權在 P0 記下 |
| emcyphal | 0.1.0 | MIT OR Apache-2.0 | 只用靜態配置，以 embassy 為主要目標；不依賴 embassy 執行器的最小用法要先確認 |
| libcanard | v5.0.0 | MIT | C；`arm-none-eabi-gcc -Os -mcpu=cortex-m7 -mthumb -mfloat-abi=hard -mfpu=fpv5-d16`；配置器回呼用固定大小的記憶池，大小另列 |

### 4.3 通過行

```
EMBNET_P0_SIZE impl=<canadensis|emcyphal|libcanard> target=thumbv7em-none-eabihf cpu=cortex-m7 text=<B> rodata=<B> data=<B> bss=<B> heap=<B> profile=z,lto run=<目錄>
```

`text` 到 `bss` 都是扣掉 baseline 後的值；`heap` 是另外配置給配置器的大小，沒有就是 0。

## 五、`vcan` 直連的處理時間

### 5.1 量法

- 同一個容器裡跑兩個 canadensis Node，一個發、一個收，經 `vcan0`。兩端用同一個 `CLOCK_MONOTONIC`。
- 發送端照固定時刻表發送，週期 1 ms；延遲從排定的發送時刻算到接收端交給應用的時刻，避免 coordinated omission（`docs/40-rust-embedded/04` 第三節）。
- 每次 100,000 筆，用 `hdrhistogram` 7.6.0 記錄，3 位有效數字；直方圖序列化後存進 run 目錄。
- 量測前在同一容器跑 60 秒 `cyclictest`（不加即時優先序），記下這台主機的排程延遲當底線；同時記下 `uname -r` 與負載。
- canadensis 是否有現成的 SocketCAN 傳輸，P0 先確認；沒有就用 `socketcan` crate 寫一層薄外層，並在 005 註明。

這裡量到的只是軟體處理時間，沒有 Wire time（`003` K-32）。

### 5.2 通過行

```
EMBNET_P0_VCAN impl=canadensis env=host period_us=1000 samples=<N> p50_us=<N> p99_us=<N> p99_ci_us=<低>-<高> max_us=<N> run=<目錄>
```

`p99_ci_us` 依 `docs/40-rust-embedded/04` 第二節的排序統計量計算。樣本少於 10,000 時印 `INSUFFICIENT`，不印 `PASS`。

## 六、Renode

### 6.1 量法

- 平台：`platforms/boards/nucleo_f767zi.repl`。它引用 `stm32f777.repl`，裡面有三個 bxCAN、乙太網路 MAC、DMA 與 DWT。
- 程式：一個只做 CAN3 loopback 的最小韌體（`probes/p0-renode`），送一框、收一框、比對內容。embassy 的計時器改用 TIM2，預設的 TIM12 不在平台描述裡。
- 額外檢查兩件已知的限制，確認它們在實際執行時也成立（`003` K-31）：
  - 同時放三個 mailbox、ID 由大到小，看送出順序是否依 ID（模型原始碼顯示 TXFP 只存不用）。
  - 讀 DWT CYCCNT 的增量，對照設定的 72 MHz。
- 執行：`renode --disable-xwt --console`，加 Robot 測試或 monitor 腳本，在容器裡以 `--network none` 跑。

### 6.2 通過行

```
EMBNET_P0_RENODE platform=nucleo_f767zi renode=<版本> can_loopback=<PASS|FAIL> tx_order_by_id=<yes|no> dwt_hz=<N> run=<目錄>
```

## 七、授權與 bridge 參考實作

- `tools/p0-license.sh` 用 `cargo metadata` 列出每個 probe 的全部相依與 license 欄位，標出不是 MIT 或 Apache-2.0 的項目。
  通過行：`EMBNET_P0_LICENSE crates=<N> non_permissive=<N> unknown=<N> run=<目錄>`。`non_permissive` 與 `unknown` 不為 0 時逐項寫進 005。
- cyphal-bridge-and-router：只讀原始碼，記下它怎麼處理 Transfer-ID 與來源 Node-ID，寫進 005，不沿用程式碼（K-12）。

## 八、之後才需要的硬體

P0 不需要，P3 之前要備齊：

| 項目 | 數量 | 原因 |
|---|---|---|
| NUCLEO-F767ZI | 2 | 一台當 Bridge，一台當 Native node |
| 3.3 V CAN 收發器模組（例如 SN65HVD230） | 每條 CAN 一個，至少 4 | 板上沒有收發器（K-33） |
| 120 Ω 終端電阻 | 每條匯流排 2 | 匯流排兩端 |
| USB 轉 UART | 1 | UART Link 與除錯 |
| 邏輯分析儀（可解 CAN） | 1 | 線到線延遲（`docs/40-rust-embedded/04` 第 5.2 節） |

腳位規劃要對照 UM1974：CAN2 的 PB12/PB13 中，PB13 被板上乙太網路的 RMII 占用，要改用 PB5/PB6；PB5/PB6 與 CAN3 腳位在板上是否空著，待查證。
ST 的範例也提到用 bxCAN 時要把 ST-LINK 的 8 MHz 時脈接到 HSE（焊上 SB149），這點在 P5 前確認。

## 九、執行順序

1. 確認 `vcan` 能在容器的 network namespace 建立（延後）。
2. 建 `embnet-rust`、`embnet-py` 映像，記下 digest。這一步要網路。
3. 抓三個 probe 的相依（`NET=bridge tools/cargo.sh ... fetch`），之後一律離線。
4. 體積：baseline、emcyphal、canadensis、libcanard。
5. `vcan` 處理時間（延後）。
6. Renode。
7. 授權清單與 bridge 參考實作的筆記。
8. 寫 `005-prior-art.md`，把 `001` 第六節的「沿用、改良或只參考」逐項填上結論。

## 十、邊界

- 一律 `docker run --rm`，帶 `--user $(id -u):$(id -g)`、`--cpus 4`、`--memory 4g`、`--pids-limit 1024`、`--log-opt max-size=10m --log-opt max-file=3`。
- 只建立 `embnet-` 前綴的映像與 volume；不執行任何 prune 或 rmi，不碰其他映像。
- 原始碼唯讀掛載；寫入只到 `runs/` 與 `target/`。
- 主機上需要 root 的動作（載入模組）由使用者執行。

## 來源

- Rust 1.100.0：<https://blog.rust-lang.org/releases/>；`thumbv7em-none-eabihf` 與 `target-cpu=cortex-m7`：<https://doc.rust-lang.org/rustc/platform-support/thumbv7em-none-eabi.html>
- STM32F767ZI 的周邊清單：embassy `stm32-data-generated` 的 `STM32F767ZI.json`；DS11532 第 3.33 節（bxCAN）；RM0410 第 2.1.6 節（DTCM 與 DMA）
- canadensis：<https://github.com/samcrow/canadensis>（`canadensis/src/lib.rs`、README）；emcyphal：<https://github.com/dan-stefanov/emcyphal>；libcanard：<https://github.com/OpenCyphal/libcanard>
- pycyphal 1.27.1：PyPI；測試的 Python 版本見 `master-v1` 分支的 `noxfile.py`
- Renode：<https://github.com/renode/renode>（`platforms/boards/nucleo_f767zi.repl`、`platforms/cpus/stm32f777.repl`）；
  bxCAN 模型：<https://github.com/renode/renode-infrastructure>（`src/Emulator/Peripherals/Peripherals/CAN/STMCAN.cs`）
- NUCLEO-F767ZI 的 CAN 收發器與時脈：ST 社群〈Using CAN (bxCAN) in Normal mode with STM32 microcontrollers〉，2025-06-25
