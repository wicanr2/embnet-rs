# 07 設定檔與建置期檢查

版本 0.2。設定檔是 TOML，`embnet-config` 讀它、做第三節的檢查，通過才能建置。轉送表、ID 與型別雜湊都在建置期固定（`CLAUDE.md`；`docs/30-safety-security/03`）。

## 一、格式

```toml
version = 1

[[link]]
name = "can1"
kind = "can"            # can、can-fd、uart、udp
bitrate = 1_000_000     # can、can-fd：仲裁段 bit/s；uart：baud；udp：bit/s
zone = "machine"        # board、machine、external
util_limit = 0.5        # 可省略，預設 0.5
max_payload = 256       # 只有 uart 需要

[[node]]
name = "bridge1"
id = 1
links = ["eth0", "can1"]
bridge = true           # 可省略，預設 false
latency_us = 200        # 只有 bridge 需要：處理時間的上限，也就是服務曲線的 T
cpu_hz = 216_000_000    # 可省略：C10 用，預設參考板的 216 MHz
cpu_budget = 0.5        # 可省略：可用來驗證 MAC 的 CPU 比例
mac_verify_cycles = 8_518  # 可省略：驗證一則 MAC 的週期數，預設為實板量測值

[[type]]
name = "demo.Speed"
major = 1
max_len = 4             # 序列化後的最大長度，不含 E2E 或 MAC 的開銷
fields = "int16 left; int16 right"

[[subject]]
name = "speed_cmd"
id = 100
type = "demo.Speed"
priority = 2
publisher = "host"
period_us = 10_000
jitter_us = 500         # 可省略，預設 0
class = "protected"     # normal、protected
protection = "e2e32"    # protected 才需要：e2e32、mac32、mac64
key = 1                 # mac32、mac64 才需要
deadline_us = 50_000    # protected 才需要
max_delta = 3           # 可省略，預設 3
drop = "newest"         # oldest、newest；可省略，預設 oldest
tid_timeout_us = 1_000_000  # 可省略，預設 1 秒

[[service]]
name = "set_mode"
id = 10
request_type = "demo.Mode"
response_type = "demo.Ack"
server = "motor"
priority = 3
timeout_us = 100_000    # 可省略，預設 1 秒

[[route]]
subject = "speed_cmd"   # 或 service = "..."
bridge = "bridge1"
from = "eth0"
to = "can1"
```

## 二、Type hash

Type hash 是下列字串的 CRC-64/XZ，以 little-endian 的 8 B 參與 E2E 與 MAC 的計算（`02-e2e.md` 第二節）：

```
<name>:<major>:<max_len>:<fields 正規化>
```

`fields` 的正規化：連續空白縮成一個空白，去掉前後空白，`;` 前後的空白去掉。minor 版本不參與，minor 之間必須相容（`docs/20-protocol/01` 第八節）。

CRC-64/XZ：poly 0x42F0E1EBA9EA3693、init 全 1、refin 與 refout 是、xorout 全 1、check 0x995DC9BBDF1939FA。

## 三、檢查

| 編號 | 檢查 | 依據 |
|---|---|---|
| C1 | 名稱唯一；Node-ID 0 到 127 且唯一；應用的 Subject-ID 0 到 7999、Service-ID 0 到 499 且唯一 | `03-node-services.md` 第一節 |
| C2 | 引用的 Link、Node、型別都存在；Bridge 有 `latency_us` | |
| C3 | `protected` 有 `protection` 與 `deadline_us`；`mac32`、`mac64` 有 `key` | `002` 第 2.3 節 |
| C4 | `tid_timeout_us` 小於 32 個發佈週期 | `01-can-transport.md` 第六節 |
| C5 | 轉送：Bridge 同時接在 `from` 與 `to` 上；`from` 上確實有這個 Subject | |
| C6 | 沒有迴路，而且每個 Subject 進入每條 Link 的路徑唯一（含發佈者自己所在的 Link） | `003` K-27 |
| C7 | 每條 Link 的使用率 Σr/R 不超過 `util_limit` | `docs/20-protocol/04` 第三節 |
| C8 | 每個 `protected` Subject 從發佈者到它出現的每條 Link，沿路徑把各段上限相加，不超過 `deadline_us` | `docs/20-protocol/04` 第 3.3 節、第八節 |
| C9 | `uart` 的 `max_payload` 不小於經過它的最大 Transfer；`udp` 不超過 1460 B | `04-datagram-links.md` |
| C10 | 每個 Bridge 每秒要驗證的 MAC 數（跨 Trust zone 轉送的 `mac32`、`mac64` Subject）乘上 `mac_verify_cycles`，不超過 `cpu_hz` × `cpu_budget` | `003` K-05；`docs/plans/007` 第 5.1 節 |

### C7、C8 的算法

每個 Subject 在一條 Link 上：

- 每筆 Transfer 的長度 L = `max_len` + 保護開銷（normal 0、e2e32 與 mac32 5、mac64 9）。
- 每筆的位元數 B：
  - can：依 `01-can-transport.md` 分框，每框 n 個資料位元組的最壞框長是 80 + 10n 位元（延伸框、最壞填充，`docs/10-buses/01` 第四節）。
  - can-fd：資料段速率另計，0.2 版先以仲裁段速率保守估計。
  - uart：datagram 長度 L + 12，COBS 後最多加 ⌈(L + 12)/254⌉ + 1 B，每位元組 10 位元（8N1）。
  - udp：每個 datagram 在乙太網路上占 max(L + 12 + 8 + 20 + 38, 84) B，每位元組 8 位元。
- 到達曲線：r = B / 週期，b = B × (jitter / 週期 + 1)。
- 服務曲線：R = `bitrate`。T = 最長的較低優先序框的時間（非搶占阻擋）；這條 Link 若由 Bridge 送出，再加上該 Bridge 的 `latency_us`。
- 優先等級 p 的上限：令 r_H、b_H 為優先序較高者的總和，R' = R − r_H，T' = T + (b_H + r_H·T)/R'，上限 = T' + b_p / R'，b_p 是同一等級的總突發量。
- R' ≤ 0 時檢查失敗。
- 端到端：沿轉送路徑把各段的上限相加。逐段相加比把整條路徑當成一個節點計算（突發量只付一次）保守。

## 四、輸出

```
EMBNET_CONFIG PASS file=<路徑> checks=<N> links=<N> subjects=<N> routes=<N> max_util=<N>% worst_bound_us=<N>
```

失敗時每一條違規印一行 `EMBNET_CONFIG_FAIL <檢查編號> <說明>`，最後印 `EMBNET_CONFIG FAIL ...`，結束碼不為 0。
