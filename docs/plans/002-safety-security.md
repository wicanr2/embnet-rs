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
| 延遲過大 | 低優先序資料卡在佇列 | 每個 Subject 設定期限。預設以接收端本地的接收間隔判斷；各 Node 的時鐘沒有同步，需要絕對時間的 Subject 才加對時（見 `003` K-03） |
| 插入 | 不該出現的 Transfer | Bridge 只轉送轉送表內的 Transfer，其餘丟棄 |
| 冒充 | 錯誤的 Node 送出某 Subject | 轉送表綁定來源 Node-ID 與來源 Link；要對抗惡意時改用 MAC（第四節） |
| 定址錯誤 | ID 設定衝突 | 建置期檢查 ID 唯一性與 Type hash |

### 2.2 Node 與 Bridge 本身的故障

| 故障 | 對策 |
|---|---|
| Node 當機 | Heartbeat 逾時，訂閱端與 Bridge 都能發現 |
| Bridge 當機或卡死 | 硬體看門狗偵測卡死。panic 時停止所有 Link 的輸出（fail-silent），取消控制器中尚未送出的框，不送出半成品 |
| panic 之後的紀錄 | 把 panic 的位置與訊息寫進重設後仍保留的 RAM 區；平台有預留快閃區時再複製過去，只保留最近幾筆，避免耗盡寫入壽命 |
| 通知上位系統 | 快閃中有未讀的異常紀錄時，Heartbeat 帶一個「有未讀的異常 log」旗標；上位系統看到後用 Service 讀取紀錄，讀完再用 Service 清除旗標。欄位與 Service 的格式在 P1 定義 |
| panic 之後的恢復 | 由部署設定選擇：停住並持續餵看門狗、等人處理；或讓看門狗逾時重設，開機時讀回紀錄，連續重設超過上限就改為停住。同匯流排的 Node 不受影響，只有跨 Link 的轉送中斷 |
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
| 保留位元（r0、r1） | 1 到 2 | 不可用 | 由控制器硬體產生，軟體寫不到；CAN FD 把這個位置拿去當 FDF（標準框是原 r0、延伸框是原 r1），改了會讓接收端誤判框型 |
| CRC、ACK、EOF | 無 | 不可用 | 由硬體產生與檢查 |
| DLC 9 到 15 | 約 3 | 不建議 | classic CAN 中這些值都代表 8 B，各控制器是否原樣收送不一致，位元也太少 |
| 29 位元 ID 剩下的位元 | Subject 5、Service 1（見 `001` 5.1） | 可用，當作 MAC 的延伸 | 放在 ID 最低位、來源 Node-ID 之下。只有來源與 Subject 都相同的框才會比到這幾位，而這些框來自同一個 Node，不會互相競爭仲裁。接收端的過濾器要遮掉這幾位 |
| Tail byte 的 Transfer-ID | 5 | 不當 FV | Bridge 轉送時可能改寫（`003` K-13），也太短（K-16）；FV 改由 E2E 序號兼任 |
| 沒用完的資料 bytes | 0 到 7 | 主要做法 | 截短 MAC 放這裡，做法同 AUTOSAR SecOC |

結論：只用未使用的位元放不下足夠的 MAC。ID 剩下的 5 位元若單獨當 MAC，攻擊者平均猜 32 次就能成功；
在 1 Mbit/s 下只要幾毫秒。這些位元適合用來延伸放在資料區的 MAC。

### 4.3 截短 MAC 的強度由失敗次數上限決定

隨機猜一個 n 位元的 MAC，猜中的機率是 2⁻ⁿ。所以截短 MAC 的安全性取決於「攻擊者能猜幾次」。
NIST SP 800-38B 附錄 A.2 給出條件：n ≥ lg(容許的驗證失敗次數 ÷ 可接受的偽造風險）。
風險取 2⁻²⁰ 時，同一把金鑰在所有接收端合計能容許的失敗次數是：

| MAC 長度 | 容許的失敗次數 |
|---|---|
| 24 位元（SecOC Profile 1、2） | 16 |
| 28 位元（SecOC Profile 3） | 256 |
| 32 位元 | 4,096 |
| 64 位元 | 約 1.8 × 10¹³ |

NIST 規定 64 位元以下的 tag 只能在協定確實限制失敗次數、並經風險分析後使用；Ascon 的規範（SP 800-232 第 4.3 節）要求 tag 至少 32 位元。
推導與圖見 `docs/30-safety-security/02`。

所以截短 MAC 一定要搭配失敗計數：每個接收端分配到一部分失敗預算，用完就發警報、把該 Subject 判為 INVALID，並要求換金鑰。
門檻與換金鑰的流程在 P1 決定。

### 4.4 CAN 上的配置候選

E2E 序號兼任 FV 的低位（做法同 SecOC 的 `SecOCUseAuthDataFreshness`），完整 FV、Data ID、Type hash 只參與 MAC 計算，不上線。

| 方案 | 每框開銷 | 單框可放資料 | MAC 長度 | 代價 |
|---|---|---|---|---|
| A：classic CAN，每框 MAC 32 位元 | Tail 1 B + 序號 1 B + MAC 4 B | 2 B | 32 位元，容許約 4,096 次失敗 | 資料很少；長資料要多框 |
| B：classic CAN，每個 Transfer 一個 32 位元 MAC，放在最後一框 | 最後一框多 5 B | 多框攤提 | 32 位元 | 要收齊整筆才能驗證 |
| C：CAN FD，每框 MAC 64 位元 | Tail 1 B + 序號 2 B + MAC 8 B | 53 B | 64 位元，達到 NIST 對多數應用的建議 | bxCAN 不支援 CAN FD，要換有 FDCAN 的晶片 |
| D：classic CAN，每框 MAC 24 位元 | Tail 1 B + 序號 1 B + MAC 3 B | 3 B | 24 位元，只容許約 16 次失敗 | 必須有換金鑰的機制；低於 Ascon 規範的下限 |

建議以 C 為主、A 或 B 為 classic CAN 的替代，D 只在有換金鑰機制時才考慮。所有方案都需要：

- 定期送一個完整 FV 的同步 Transfer，讓漏收過的接收端能重新對上；重建失敗的嘗試也算進失敗預算。
- 完整 FV 跨重開機仍要單調遞增：寫入非揮發記憶體，或每次開機衍生新的 session 金鑰（`003` K-17）。
- STM32F427 沒有硬體 AES（RM0090 第 23 章只適用於 F415/417 與 F43x），要用軟體 AES-CMAC；週期數見 `003` 第 1.2 節。
- Bridge 在 Trust zone 邊界持有跨 zone Subject 的金鑰，驗證後才轉入內部；同一 zone 內的 MAC 由收發兩端驗證，Bridge 只轉送。
- 對稱金鑰在一對多廣播時，任何持有金鑰的接收端都能偽造發送端（`003` K-18）。

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
| 畸形輸入 | 每個解析器 fuzz 固定時數；覆蓋率用 `cargo fuzz coverage` 對語料庫重跑 | `EMBNET_FUZZ target=<名稱> hours=<N> corpus=<N> cov_metric=<line\|region> cov=<N>% crashes=0` |
| 通訊故障 | 在模擬 Link 以固定 seed 注入遺失、重複、亂序、毀損、延遲，檢查接收端的判定 | `EMBNET_FAULT PASS kind=<類型> seed=<N> injected=<N> detected=<N>` |
| 偽造 | 以隨機 MAC 送假框，檢查全部被拒且計數正確 | `EMBNET_FORGE PASS sent=<N> accepted=0 counted=<N>` |
| FV 同步 | 讓接收端連續漏收超過截短 FV 的範圍，檢查重新同步所需時間與用掉的失敗預算 | `EMBNET_FV_RESYNC recover_ms=<N>` |
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
