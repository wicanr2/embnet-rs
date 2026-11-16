# 002 Safety 與 Security

狀態：草案，待審查，P1 定案。術語依 `CONTEXT.md`。

Safety 處理「系統自己出錯時不造成危害」，Security 處理「有人故意讓系統出錯」。
兩者共用一部分機制（例如 E2E 標頭、資源上限），但威脅來源不同，所以分開列。

## 一、立場與範圍

- 本專案不提供功能安全認證。參考的標準：IEC 61508（功能安全通則）、ISO 26262（道路車輛）、
  IEC 61784-3（現場匯流排的功能安全通訊）、IEC 62443（工業控制系統資安）、AUTOSAR 的 E2E 與 SecOC。
  這裡只借用它們的故障分類與防護手法，不宣稱符合。
- **Router 預設不是安全功能。** 急停、馬達過流保護這類安全迴路要留在本地或硬接線上，不經過 Router。
  原因是 Router 把多條 Link 集中到一處，它一出錯，所有經過它的訊息都受影響，等於單點故障加上共因故障。
- Topic 分成兩級，在設定檔標註：
  - `normal`：一般資料。
  - `protected`：錯了會造成設備或人員風險的資料，例如運動指令。強制使用 E2E 標頭與期限檢查，接收端自己判斷資料能不能用。

## 二、Safety

### 2.1 通訊故障與對策

故障分類依 IEC 61784-3 的通訊錯誤類型。對策的原則是：發送端加上保護，接收端檢查。
Router 只轉送、不改寫 E2E 標頭，所以 Router 本身出錯也能被接收端發現。

| 故障 | 例子 | 對策 |
|---|---|---|
| 資料毀損 | SPI、I²C 沒有鏈路 CRC；Router 記憶體錯誤 | E2E 標頭的 CRC，涵蓋 Data ID 與 payload |
| 重複 | 重傳或迴路造成同一筆收兩次 | E2E 序號，接收端拒收重複或回退的序號 |
| 遺失 | 佇列滿、鏈路斷線 | 序號跳號計數；`protected` topic 的期限檢查 |
| 順序錯亂 | 分片在不同路徑上重組 | 序號；分片重組有上限與逾時 |
| 延遲過大 | 低優先序資料卡在佇列 | 每筆帶時戳或年齡，接收端依 topic 設定的期限判斷是否過期 |
| 插入 | 不該出現的訊息 | Data ID 與 Routing table 比對，不在表內的一律丟棄 |
| 冒充 | 錯誤的 Node 送出某 topic | Data ID 加上 Route 綁定來源 Link；需要對抗惡意時改用 MAC（見第三節） |
| 定址錯誤 | Topic ID 設定衝突 | 建置期由 `embnet-config` 檢查 ID 唯一性與型別雜湊 |

### 2.2 Router 本身的故障

| 故障 | 對策 |
|---|---|
| 當機或卡死 | 硬體看門狗；當機時停止發話（fail-silent），不送出半成品 |
| 重啟後狀態遺失 | Routing table 在建置期固定，重啟後不需要 Node 重新註冊就能恢復轉送；session 序號由 Node 重新同步 |
| 某個 Node 狂發訊息（babbling idiot） | 每個 Endpoint 在 Router 入口限流（token bucket），超量就丟棄並累計計數 |
| 一條慢 Link 拖住其他 Link | 每條出口 Link 有獨立的有界佇列，不共用 |
| 佇列滿時該丟哪筆 | 每個 topic 在設定檔指定：狀態類丟最舊的，指令類丟最新的並回報 |
| 執行時間不可預期 | 核心不配置記憶體、佇列與迴圈都有上限，最壞情況處理時間可以量測 |

### 2.3 設定錯誤

設定錯誤在建置期擋下，不留到執行時：

- Topic ID 與 Data ID 唯一。
- 發送端與接收端的型別雜湊一致。
- 訊息長度加 E2E 標頭後，分片數不超過 Link 的上限。
- `protected` topic 一定有期限與 E2E 設定。
- Routing table 中沒有迴路。

## 三、Security

### 3.1 資產與信任邊界

要保護的資產：控制指令的完整性、Router 的可用性、金鑰，以及部分感測資料的機密性（是否需要待審查）。

Trust zone 的初步劃分：

| Trust zone | Link | 理由 |
|---|---|---|
| 板內 | SPI、I²C | 要碰到必須打開機殼 |
| 機內 | UART、RS-485、CAN、Modbus | 線材在設備內，但維修時可接觸，也可能有第三方模組 |
| 對外 | Ethernet / UDP | 可能接上工廠網路或維修筆電，暴露面最大 |

**Router 會造出新的攻擊路徑。** 原本 CAN 與乙太網路互不相通，接上 Router 後，從乙太網路進來的攻擊者
可能把訊息送進 CAN。所以跨 Trust zone 的轉送一律預設拒絕，只有 Routing table 明列的 Route 才放行，
而且方向要分開列。

### 3.2 威脅（STRIDE）

| 類別 | 威脅 | 對策 |
|---|---|---|
| 冒充（Spoofing） | 從乙太網路假冒某個 Node 送運動指令 | 對外 Trust zone 的 Link 強制認證；跨 Trust zone 的 `protected` topic 帶 MAC |
| 竄改（Tampering） | 中間人修改 payload | MAC 涵蓋 Data ID、序號與 payload |
| 重放（Replay） | 錄下合法指令事後重送 | MAC 加新鮮度值（freshness value，做法參考 AUTOSAR SecOC） |
| 資訊洩漏（Information disclosure） | 竊聽感測資料 | 對外 Link 可選加密；板內與機內先不加密（待審查） |
| 阻斷服務（DoS） | 洪泛乙太網路埠；在 CAN 上故意製造錯誤讓節點進入 bus-off | 入口限流與有界佇列；Link 之間隔離；CAN 的錯誤計數納入健康監控 |
| 權限提升（Elevation of privilege） | 解析器漏洞讓攻擊者在 Router 上執行程式 | 核心用 Rust 且 `forbid(unsafe_code)`；解析路徑 fuzz；不支援執行時下載設定 |

否認（Repudiation）這一類先不處理，只保留 security 事件計數（認證失敗、限流丟棄、未知 Topic ID），
並把計數以 topic 的形式發佈。

### 3.3 認證的長度預算

MAC 會吃掉 payload 空間。CAN classic 一框只有 8 B，放不下完整的 MAC 和新鮮度值。可能的做法：

- 截短 MAC 並搭配截短的新鮮度值（AUTOSAR SecOC 的做法），代價是強度降低。
- `protected` topic 只走 CAN FD 或乙太網路。
- 板內與機內不做 MAC，只靠 Trust zone 邊界。

P1 要依 `001` 第九節的參考板與機密性需求，用數字決定。

### 3.4 金鑰

- 金鑰的產生、注入與輪替屬於平台的責任（安全開機、韌體更新也是）。本專案只定義介面，並假設平台提供安全的金鑰儲存。
- repo 內只有測試用金鑰，檔名標 `test-only`。

## 四、程式層面的要求

Safety 與 security 共用：

- 核心 crate：`#![forbid(unsafe_code)]`；啟用 clippy 的 `unwrap_used`、`expect_used`、`indexing_slicing`、`arithmetic_side_effects`；
  解析路徑不 panic。
- 鏈路 crate 的 `unsafe`：每處都寫不變式，並在主機測試下用 Miri 檢查能檢查的部分。
- 每個解析器都有 fuzz target，報告附覆蓋率。
- 序號、分片重組、限流這類狀態機，用性質測試（property test）檢查不變式。是否加上模型檢查（例如 Kani），P1 決定。
- 供應鏈：`Cargo.lock` 進版控；用 `cargo-deny` 檢查授權與已知漏洞；相依的授權記在 `docs/`。

## 五、驗證

| 項目 | 方法 | 通過行 |
|---|---|---|
| 畸形輸入 | 每個解析器的 fuzz，跑固定時數 | `EMBNET_FUZZ target=<名稱> hours=<N> cov=<N>% crashes=0` |
| 通訊故障 | 在 Link 層注入遺失、重複、亂序、毀損、延遲，檢查接收端的判定 | `EMBNET_FAULT PASS kind=<類型> injected=<N> detected=<N>` |
| 洪泛隔離 | 一條 Link 以線速洪泛，量其他 Link 的 p99 | `EMBNET_FLOOD link=<名稱> other_p99_delta_us=<N>` |
| 跨 Trust zone | 未列入 Routing table 的轉送一律被拒 | `EMBNET_ZONE PASS attempts=<N> forwarded=0` |
| 重啟 | Router 重啟後恢復轉送所需時間 | `EMBNET_RESTART recover_ms=<N>` |

## 六、待審查決定

| 問題 | 影響 |
|---|---|
| 是否有 topic 需要 `protected` 等級，預期的安全等級 | E2E 標頭長度、期限檢查，以及要不要做雙通道 |
| 板內與機內 Link 是否需要認證或加密 | MAC 長度預算、CAN classic 是否可用 |
| 對外 Link 用什麼認證協定 | 候選要在 P1 依 MCU 體積與延遲比較，目前不預設 |
| Router 重啟期間的行為 | 接收端在期限過後要進入什麼狀態，由應用端定義還是由本專案提供預設 |
