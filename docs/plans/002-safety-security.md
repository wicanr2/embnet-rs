# 002 Safety 與 Security

狀態：草案，待審查，P1 定案。術語依 `CONTEXT.md`。

Safety 處理系統自己出錯的情況：線路雜訊、記憶體錯誤、某個 Node 失控。Security 處理有人故意讓系統出錯的情況。
兩者共用一部分機制，例如 E2E 保護和資源上限，但威脅來源不同，所以分開寫。

## 一、立場與範圍

- 本專案不提供功能安全認證。這裡借用下列標準的故障分類與防護手法，不宣稱符合：
  IEC 61508（功能安全通則）、ISO 26262（道路車輛）、IEC 61784-3（現場匯流排的功能安全通訊）、
  IEC 62443（工業控制系統資安）、AUTOSAR 的 E2E 與 SecOC。
- **Bridge 預設不擔任安全功能。** 急停、馬達過流保護這類安全迴路留在本地或硬接線上。
  Bridge 是多條 Link 的交會點，它出錯時所有經過它的 Transfer 都受影響。
  沿用 Cyphal 的去中心化設計有一個好處：同匯流排上的 Node 直接互通，Bridge 故障不會中斷同匯流排內的通訊。
- Subject 分成兩級，在設定檔標註：
  - `normal`：一般資料。
  - `protected`：錯了會造成設備或人員風險的資料，例如運動指令。強制使用 E2E 保護與期限檢查，
    接收端自己判斷資料能不能用。

## 二、Safety

### 2.1 通訊故障與對策

故障分類依 IEC 61784-3。原則是發送端加保護、接收端檢查。Bridge 轉送時不改寫 E2E 保護欄位，
所以 Bridge 自己出錯，接收端也能發現。

| 故障 | 例子 | 對策 |
|---|---|---|
| 資料毀損 | SPI、I²C 沒有鏈路 CRC；Bridge 記憶體錯誤 | E2E CRC。Subject-ID 與 Type hash 一起算進 CRC，但不上線傳送（做法同 AUTOSAR E2E 的 Data ID） |
| 重複 | 重傳或迴路造成同一筆收兩次 | Transfer-ID，接收端拒收重複或回退的值 |
| 遺失 | 佇列滿、鏈路斷線 | Transfer-ID 跳號計數；`protected` Subject 的期限檢查 |
| 順序錯亂 | 多框 Transfer 的框交錯 | Tail byte 的 toggle 與 Transfer-ID；重組有上限與逾時 |
| 延遲過大 | 低優先序資料卡在佇列 | 每個 Subject 設定期限，接收端依時戳判斷是否過期 |
| 插入 | 不該出現的 Transfer | Bridge 只轉送轉送表內的 Transfer，其餘丟棄 |
| 冒充 | 錯誤的 Node 送出某 Subject | 轉送表綁定來源 Node-ID 與來源 Link；要對抗惡意時改用 MAC（第四節） |
| 定址錯誤 | ID 設定衝突 | 建置期檢查 ID 唯一性與 Type hash |

### 2.2 Node 與 Bridge 本身的故障

| 故障 | 對策 |
|---|---|
| Node 當機 | Heartbeat 逾時，訂閱端與 Bridge 都能發現 |
| Bridge 當機或卡死 | 硬體看門狗；當機時停止發話（fail-silent），不送出半成品 |
| Bridge 重啟後狀態遺失 | 轉送表在建置期固定，重啟後不需 Node 重新註冊就能恢復；重組中的 Transfer 丟棄 |
| 某個 Node 狂發（babbling idiot） | 在 CAN 上由 Priority 限制它對高優先序流量的影響；Bridge 入口對每個（來源、Subject）限流 |
| 一條慢 Link 拖住其他 Link | 每條出口 Link 有獨立的有界佇列 |
| 佇列滿時丟哪筆 | 每個 Subject 在設定檔指定：狀態類丟最舊的，指令類丟最新的並計數 |
| 執行時間不可預期 | 核心不配置記憶體，佇列與迴圈都有上限，最壞處理時間可以量測 |

### 2.3 設定錯誤在建置期擋下

- Subject-ID、Service-ID、Node-ID 唯一。
- 收發兩端的 Type hash 一致。
- 每條 Link 上，Transfer 加上保護欄位後的框數不超過上限。
- `protected` Subject 一定設定了期限與 E2E 保護。
- 轉送表沒有迴路。

## 三、Security

### 3.1 信任邊界

要保護的資產：控制指令的完整性、Bridge 的可用性、金鑰，以及部分感測資料的機密性（是否需要待審查）。

| Trust zone | Link | 理由 |
|---|---|---|
| 板內 | SPI、I²C | 要碰到必須打開機殼 |
| 機內 | CAN、UART、RS-485、Modbus | 線材在設備內，但維修時可接觸，也可能接上第三方模組 |
| 對外 | Ethernet / UDP | 可能接上工廠網路或維修筆電，暴露面最大 |

接上 Bridge 之後，原本互不相通的乙太網路和 CAN 就有了通路。所以 Bridge 同時是防火牆：
跨 Trust zone 的轉送預設拒絕，只有轉送表明列的 Subject 才放行，兩個方向分開列。

### 3.2 威脅（STRIDE）

| 類別 | 威脅 | 對策 |
|---|---|---|
| 冒充 | 從乙太網路假冒某個 Node 送運動指令 | 對外 Link 強制認證；跨 Trust zone 的 `protected` Subject 帶 MAC |
| 竄改 | 中間人修改 payload | MAC 涵蓋 ID、FV 與 payload |
| 重放 | 錄下合法指令事後重送 | MAC 計算納入 FV |
| 資訊洩漏 | 竊聽感測資料 | 對外 Link 可選加密；板內與機內先不加密（待審查） |
| 阻斷服務 | 洪泛乙太網路埠；在 CAN 上故意製造錯誤讓節點進入 bus-off | 入口限流與有界佇列；Link 之間隔離；CAN 錯誤計數納入 Heartbeat 的健康狀態 |
| 權限提升 | 解析器漏洞讓攻擊者在 Bridge 上執行程式 | 核心用 Rust 且 `forbid(unsafe_code)`；解析路徑 fuzz；不支援執行時下載設定 |

否認這一類先不處理。Security 事件（MAC 錯誤、限流丟棄、未知 ID）以計數的形式隨 Heartbeat 或專用 Subject 發佈。

## 四、CAN 上的認證

### 4.1 先分清楚要防什麼

| 想要的效果 | 需要的機制 | CAN 現況 |
|---|---|---|
| 偵測線路雜訊造成的錯誤 | CRC | CAN 硬體已有 CRC-15，CAN FD 有更長的 CRC |
| 偵測 Bridge 或記憶體造成的錯誤 | E2E CRC | 2.1 節已列 |
| 防止有人偽造或竄改 | 以金鑰計算的 MAC，再加 FV 防重放 | 需要另加 |
| 防止竊聽 | 加密 | 多數內部匯流排不做 |

不帶金鑰的雜湊（一般所說的指紋）攻擊者也算得出來，只能防雜訊，而這部分 CAN 的 CRC 已經做了。
要確保訊息真的來自合法的 Node，必須用 MAC。

### 4.2 哪些位元可以利用

classic CAN（包括 STM32F4 的 bxCAN）每框只有 8 B 資料，所以值得把每個沒用到的位元都檢查一遍：

| 位置 | 可用位元 | 判斷 | 原因 |
|---|---|---|---|
| 保留位元（r0、r1） | 1 到 2 | 不可用 | 由控制器硬體產生，軟體寫不到；CAN FD 把 r0 的位置拿去當 FDF，改了會讓接收端誤判框型 |
| CRC、ACK、EOF | 無 | 不可用 | 由硬體產生與檢查 |
| DLC 9 到 15 | 約 3 | 不建議 | classic CAN 中這些值都代表 8 B，各控制器是否原樣收送不一致，位元也太少 |
| 29 位元 ID 剩下的位元 | Subject 5、Service 1（見 `001` 5.1） | 可用，當作 MAC 的延伸 | 放在 ID 最低位、來源 Node-ID 之下。只有來源與 Subject 都相同的框才會比到這幾位，而這些框來自同一個 Node，不會互相競爭仲裁。接收端的過濾器要遮掉這幾位 |
| Tail byte 的 Transfer-ID | 5 | 可當截短 FV | 它本來就每筆遞增，不必再多佔一個 byte |
| 沒用完的資料 bytes | 0 到 7 | 主要做法 | 截短 MAC 放這裡，做法同 AUTOSAR SecOC |

結論：只用未使用的位元放不下足夠的 MAC。ID 剩下的 5 位元若單獨當 MAC，攻擊者平均猜 32 次就能成功；
在 1 Mbit/s 下只要幾毫秒。這些位元適合用來延伸放在資料區的 MAC。

### 4.3 偽造成功需要多久

假設攻擊者以最高速率送假框（1 Mbit/s 的 classic CAN 每秒約 7,600 框），Bridge 和接收端也不做任何封鎖。
每一框猜中的機率是 2^-n，平均要送 2^n 框才猜中一次：

| MAC 長度 | 平均成功時間 |
|---|---|
| 5 位元（只用 ID 剩餘位元） | 約 4 ms |
| 24 位元（SecOC Profile 1 的長度） | 約 37 分鐘 |
| 29 位元（資料區 24 位元加 ID 5 位元） | 約 20 小時 |
| 32 位元 | 約 6.5 天 |

連續送這麼多假框本身就會塞滿匯流排，而且每一框都會讓 MAC 錯誤計數增加。
所以 MAC 長度要搭配偵測：計數超過門檻就發警報，並讓接收端進入安全狀態。
門檻與反應方式在 P1 決定。

### 4.4 classic CAN 的配置候選

AUTOSAR SecOC Profile 1 用 AES-128 CMAC，截成 24 位元，再加 8 位元的截短 FV，共佔 4 B。
套到我們的框上，再借用 Tail byte 和 ID 剩下的位元：

| 方案 | 每框開銷 | 單框可放資料 | MAC 強度 | 代價 |
|---|---|---|---|---|
| A：每框 MAC 24 位元，Transfer-ID 當 FV | Tail byte 1 B + MAC 3 B | 4 B | 24 位元，加上 ID 位元可到 29 位元 | Transfer-ID 只有 5 位元，連續漏收超過 31 筆就要重新同步 FV |
| B：每個 Transfer 一個 MAC，取代多框的 CRC-16 | 最後一框多 4 B | 單框 3 B；多框攤提 | 32 位元 | 要收齊整個 Transfer 才能驗證 |
| C：`protected` Subject 只走 CAN FD | Tail byte 1 B + MAC 8 B + FV 1 B | 54 B | 64 位元 | bxCAN 不支援 CAN FD，要換有 FDCAN 的晶片 |

所有方案都需要：

- 定期送一個完整 FV 的同步 Transfer，讓漏收過的接收端能重新對上。
- 確認所選 MCU 有沒有硬體 AES。沒有的話用軟體 AES-CMAC，每框的計算時間要在 P1 量測，並算進 Bridge 處理時間。
- Bridge 在 Trust zone 邊界持有跨 zone Subject 的金鑰，驗證後才轉入內部；同一 zone 內的 MAC 由收發兩端驗證，Bridge 只轉送。

### 4.5 金鑰

- 金鑰的產生、注入與輪替，以及安全開機與韌體更新，屬於平台的責任。本專案只定義介面，並假設平台提供安全的金鑰儲存。
- repo 內只有測試用金鑰，檔名標 `test-only`。

## 五、程式層面的要求

Safety 與 security 共用：

- 核心 crate：`#![forbid(unsafe_code)]`；啟用 clippy 的 `unwrap_used`、`expect_used`、`indexing_slicing`、`arithmetic_side_effects`；
  解析路徑不 panic。
- Link crate 的 `unsafe`：每處都寫不變式，並在主機測試下用 Miri 檢查能檢查的部分。
- 每個解析器都有 fuzz target，報告附覆蓋率。
- Transfer-ID、重組、FV 同步、限流這類狀態機，用性質測試（property test）檢查不變式。要不要加模型檢查（例如 Kani），P1 決定。
- 供應鏈：`Cargo.lock` 進版控；`cargo-deny` 檢查授權與已知漏洞；相依的授權記在 `docs/`。

## 六、驗證

| 項目 | 方法 | 通過行 |
|---|---|---|
| 畸形輸入 | 每個解析器 fuzz 固定時數 | `EMBNET_FUZZ target=<名稱> hours=<N> cov=<N>% crashes=0` |
| 通訊故障 | 在 Link 層注入遺失、重複、亂序、毀損、延遲，檢查接收端的判定 | `EMBNET_FAULT PASS kind=<類型> injected=<N> detected=<N>` |
| 偽造 | 以隨機 MAC 送假框，檢查全部被拒且計數正確 | `EMBNET_FORGE PASS sent=<N> accepted=0 counted=<N>` |
| FV 同步 | 讓接收端漏收超過 31 筆，檢查重新同步所需時間 | `EMBNET_FV_RESYNC recover_ms=<N>` |
| 洪泛隔離 | 一條 Link 以線速洪泛，量其他 Link 的 p99 | `EMBNET_FLOOD link=<名稱> other_p99_delta_us=<N>` |
| 跨 Trust zone | 未列入轉送表的 Transfer 一律被拒 | `EMBNET_ZONE PASS attempts=<N> forwarded=0` |
| 重啟 | Bridge 重啟後恢復轉送所需時間 | `EMBNET_RESTART recover_ms=<N>` |

## 七、待審查決定

| 問題 | 影響 |
|---|---|
| 是否有 Subject 需要 `protected` 等級，預期的安全等級 | E2E 保護的長度、期限檢查，以及要不要做雙通道 |
| 板內與機內 Link 是否需要認證或加密 | 第四節的方案選擇，以及 bxCAN 是否夠用 |
| MAC 錯誤的門檻與反應 | 4.3 節的時間要搭配偵測才有意義 |
| 對外 Link 用什麼認證協定 | 候選要在 P1 依 MCU 體積與延遲比較，目前不預設 |
| Bridge 重啟期間接收端的行為 | 期限過後進入什麼狀態，由應用端定義或由本專案提供預設 |

## 來源

- AUTOSAR SecOC Profile 1 的參數（24 位元 CMAC、8 位元 FV）：AUTOSAR FO PRS SecOcProtocol R24-11 第 5.3.2 節，
  <https://autosar.org/fileadmin/standards/R24-11/FO/AUTOSAR_FO_PRS_SecOcProtocol.pdf>
- Cyphal/CAN 的 Tail byte 與多框 CRC：canadensis_can 原始碼，<https://docs.rs/canadensis_can/latest/src/canadensis_can/data.rs.html>
- Cyphal 跨傳輸轉送的討論與 Cyphal/CAN 的 Node-ID 上限：<https://forum.opencyphal.org/t/cyphal-udp-routing-over-multiple-networks/1657>
