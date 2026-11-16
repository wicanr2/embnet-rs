# 01 CAN 傳輸

版本 0.1。只用 29 位元延伸框。classic CAN 的 MTU 是 8；CAN FD 的規則一併定義，但參考板只有 bxCAN（`001` 第十節）。

## 一、CAN ID

### 1.1 Subject（種類位元 = 0）

| 位元 | 欄位 | 寬度 | 說明 |
|---|---|---|---|
| 28–26 | Priority | 3 | 0 最高，7 最低 |
| 25 | 種類 | 1 | 0 |
| 24–20 | 保留 | 5 | 填 0 |
| 19–7 | Subject-ID | 13 | 0 到 8191 |
| 6–0 | 來源 Node-ID | 7 | 0 到 127 |

### 1.2 Service（種類位元 = 1）

| 位元 | 欄位 | 寬度 | 說明 |
|---|---|---|---|
| 28–26 | Priority | 3 | |
| 25 | 種類 | 1 | 1 |
| 24 | request | 1 | 1 是 request，0 是 response |
| 23 | 保留 | 1 | 填 0 |
| 22–14 | Service-ID | 9 | 0 到 511 |
| 13–7 | 目的 Node-ID | 7 | |
| 6–0 | 來源 Node-ID | 7 | |

仲裁的結果：Priority 先決定；同 Priority 時 Subject 贏 Service；同一個 Subject 由來源 Node-ID 小的贏。
同一個 CAN ID 在一條匯流排上只能有一個發送者（`003` K-27），設定檔在建置期檢查。

沒有匿名 Transfer：每個 Node 都有設定好的 Node-ID。

## 二、Tail byte

每個 Frame 的最後一個位元組：

| 位元 | 欄位 | 說明 |
|---|---|---|
| 7 | SOT | 這筆 Transfer 的第一框 |
| 6 | EOT | 這筆 Transfer 的最後一框 |
| 5 | toggle | 第一框為 1，之後逐框交替 |
| 4–0 | Transfer-ID | 0 到 31 |

單框 Transfer 的 Tail byte 是 SOT=1、EOT=1、toggle=1。

## 三、分框

令 Transfer 的 payload 長度為 L，每框可帶的 payload 是 M = MTU − 1。

- **L ≤ M：單框。** Frame 的資料是 payload 加上 Tail byte，長度 L + 1。不加 CRC。
- **L > M：多框。** 把 payload 後面接上 CRC-16/IBM-3740（big-endian，2 B），得到長度 L + 2 的串流，依序切成每段 M 位元組，最後一段可以較短；每段後面加 Tail byte。
  CRC 計算的範圍是 payload 本身（含 CAN FD 的補位，見下一點）。
- **CAN FD 的補位：** CAN FD 的長度只能是 0–8、12、16、20、24、32、48、64。最後一框的長度不是合法值時，在 CRC **之前**補 0x00 到合法長度，補位計入 CRC。
  單框的補位放在 payload 之後、Tail byte 之前。classic CAN 不補位。
- 每框的 Transfer-ID 相同，toggle 逐框交替。

一筆 Transfer 的框數上限由設定檔的型別最大長度決定；規格本身不另設上限。

## 四、Transfer-ID

- 每個 session，也就是（種類、Subject-ID 或 Service-ID、request 位元、來源 Node-ID，Service 另加目的 Node-ID），各有一個 Transfer-ID。
- 發送端每送出一筆 Transfer 就加 1，模 32。Service 的 response 沿用 request 的 Transfer-ID。
- Transfer-ID 屬於 Link 層。跨格式轉送時由 Bridge 重新編號（`003` K-13），端到端的順序由 E2E 序號負責（`02-e2e.md`）。

## 五、重組

接收端為每個 session 保留一個重組狀態：目前的 Transfer-ID、下一個 toggle、已收的位元組、開始時間。狀態數量有上限，由設定檔決定（`003` K-14）。

收到一個 Frame 時，依序檢查：

1. 資料長度至少 1（要有 Tail byte）；保留位元是 0。否則丟棄。
2. **SOT=1：** 開始新的一筆，取代這個 session 進行中的任何狀態。toggle 必須是 1，否則丟棄。
   若同時 EOT=1（單框），直接交給第六節的去重複，再交給應用。
3. **SOT=0：** 這個 session 沒有進行中的狀態、Transfer-ID 不同、或 toggle 不符時，丟棄這一框，**保留**進行中的狀態。
4. 累積這一框的資料（不含 Tail byte）。累積的長度超過「型別最大長度 + 2」時，中止這一筆並丟棄狀態。
5. **EOT=1：** 對累積的資料算 CRC-16/IBM-3740；把最後 2 B 也算進去時結果必須是 0（等價於比對附在後面的 CRC）。
   不符就丟棄。符合就去掉最後 2 B，交給第六節的去重複，再交給應用。CAN FD 的補位由型別的解碼器忽略。
6. 每次開始新狀態時記下時間。進行中的狀態超過 Transfer-ID 逾時（第六節）就清除。

## 六、去重複與逾時

- 每個 session 記住最後一筆交給應用的 Transfer-ID 與時間。
- 新完成的 Transfer 若 Transfer-ID 與上一筆相同，且距上一筆不到 Transfer-ID 逾時，視為重複並丟棄。
- Transfer-ID 逾時由設定檔依 Subject 指定。預設 1 秒，必須小於 32 個發佈週期。

## 七、計數

接收端至少為每條 Link 計數：保留位元錯誤、toggle 錯誤、CRC 錯誤、逾時清除、重組狀態用完、重複。這些計數隨診斷 Subject 發佈（`03-node-services.md`）。
