# 006 P1 結果

狀態：P1 的程式部分完成。規格在 `docs/spec/`（0.2 版），驗證腳本是 `tools/p1-spec.sh` 與 `tools/p1-cycles.sh`。

## 一、通過行

```
EMBNET_SPEC PASS version=0.2 vectors=89 rust_tests=15 failed=0 clippy=0 thumbv7em=ok config_tests=ok config_example=PASS run=runs/20261116-183505-p1-spec
EMBNET_P1_COST env=renode-1.17.0 dwt_hz=72000000 mips=462 unit=instructions k=200 mac32_gen=17819 mac32_verify=17915 e2e32_gen=102 e2e32_verify=115 canid_decode=6 segment_9B=64 reassemble_2f=372 datagram_decode_20B=218 mac32_gen_cached=13224 mac32_verify_cached=13321 mac_key_expand=4729 run=runs/20261116-183445-p1-cycles
```

## 二、P1 的工作與對應的產出

| `001` P1 工作 | 產出 |
|---|---|
| 1. Transfer 模型、CAN ID、Tail byte、多框 CRC、Heartbeat、Service | `docs/spec/01` 到 `03` |
| 2. 跨 MTU 轉送（K-06）與新鮮度（K-03） | `docs/spec/06-forwarding.md`；資料年齡改用 Service 往返（`docs/spec/05-mac.md` 第四節） |
| 3. 保護欄位配置，選定 `002` 第四節的方案（K-01） | 參考板上用方案 A、B 的形式：`mac32` 以 Transfer 為單位；`mac64` 給 CAN FD 與 UDP（`docs/spec/05-mac.md`） |
| 4. AES-CMAC 與每一步的成本（K-05） | 第三節；在 Renode 上量，是指令數 |
| 5. 設定檔格式與建置期檢查 | `docs/spec/07-config.md`、`crates/embnet-config`（C1 到 C9，每項一個失敗案例） |
| 6. 獨立的 Python test vector 產生器（K-10） | `tools/vectors/gen.py`：10 個檔、89 個案例；AES 與 CMAC 以純 Python 實作，用 FIPS 197 與 RFC 4493 自我檢查 |

實作在 `crates/embnet-core`：CRC、CAN ID、分框與重組、E2E-32、`mac32` 與 `mac64`、datagram 與 COBS、Transfer-ID 對應、Heartbeat。全部 `no_std`、`forbid(unsafe_code)`，可以編到 `thumbv7em-none-eabihf`。

## 三、每一步的成本

在 Renode 1.17.0 的 `nucleo_f767zi` 平台上，每一步做 200 次取平均。Renode 的 DWT 以 72 MHz 對虛擬時間計數，CPU 以 462 MIPS 推進，所以表中的數字是**指令數**，由 CYCCNT 增量乘以 462/72 推算。
實機的週期數還要考慮 Cortex-M7 的雙發射、快閃記憶體等待與快取，可能比指令數少，也可能多。

| 步驟 | 指令數 |
|---|---|
| CAN ID 解碼 | 6 |
| 分框：9 B 切成 2 框 | 64 |
| E2E-32 產生（2 B 資料） | 102 |
| E2E-32 驗證 | 115 |
| datagram 解碼與 CRC-32C（20 B） | 218 |
| 重組：2 框，經 8 個 session 的表 | 372 |
| `mac32` 產生，每次展開金鑰 | 17,819 |
| `mac32` 驗證，每次展開金鑰 | 17,915 |
| `mac32` 產生，金鑰預先展開 | 13,224 |
| `mac32` 驗證，金鑰預先展開 | 13,321 |
| 金鑰展開本身 | 4,729 |

MAC 的輸入是 Data ID 11 B、序號 4 B、資料 2 B，共 17 B，要 2 個 AES 區塊；CMAC 另外在建立時算一次子金鑰。使用的是 RustCrypto `aes` 0.9.4 的軟體實作（fixslicing，文件稱為常數時間）與 `cmac` 0.8.0。

## 四、對設計的影響

- **除了 MAC，轉送一框的協定成本在幾百個指令以內。** 遠低於 `003` 第 1.2 節兩條滿載 CAN 時每框約 4,050 週期的預算。
- **軟體 MAC 是瓶頸。** 預先展開金鑰後每則仍約 13,000 個指令。若一個指令約一個週期，216 MHz 留一半餘裕時，每秒約可驗 8,000 則。
  兩條滿載 CAN 約 26,667 框/s，不可能每框都驗。對策維持 `003` K-05：
  - Bridge 只驗跨 Trust zone 的 `protected` Subject；
  - 需要更高的驗證率時改用有 CRYP 的 F777 或 F779；
  - `embnet-config` 之後加一項檢查：Bridge 每秒要驗的 MAC 數乘上每則成本，不超過 CPU 預算（建議為 C10，實板量到週期數後再定係數）。
- **預先展開金鑰值得做。** 每則省約 4,700 個指令。`mac::MacKey` 已提供這個用法。
- **Renode 的數字只能相對比較。** 實際週期數要等實板，P5 用同一支韌體 `probes/p1-cycles` 在 NUCLEO-F767ZI 上量。

## 五、還沒有答案的決定

| 問題 | 出處 | 狀態 |
|---|---|---|
| 對外 Link 的認證協定 | `002` 第七節 | 未定；UDP 可先用 `mac64`，連線層的協定在 P6 前決定 |
| `protected` Subject 的安全等級 | `002` 第七節 | 暫不訂 SIL 目標 |
| Linux 閘道是否用 PREEMPT_RT | `001` 第十節 | 已決定：用 PREEMPT_RT |
| Legacy device 的範圍 | `001` 第十節 | 未定，P4 前決定 |
| `vcan` 直連的處理時間 | `005` 第三節 | 延後 |
| embassy 計時在 Renode 上不前進 | `005` 第四節 | 已解決：平台檔缺 TIM2，以補充平台檔補上 |
