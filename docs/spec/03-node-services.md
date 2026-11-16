# 03 Heartbeat、Service 與保留的 ID

版本 0.1。

## 一、保留的 ID

| 種類 | ID | 用途 | Priority |
|---|---|---|---|
| Subject | 8191 | 保留 | |
| Subject | 8190 | Heartbeat | 4 |
| Subject | 8189 | E2E Sync（`02-e2e.md` 第五節） | 4 |
| Subject | 8188 | 診斷計數（0.2 版定義格式） | 6 |
| Subject | 8000 到 8187 | 保留給協定 | |
| Service | 510 | 讀取異常紀錄（格式在 0.2 版定義） | 5 |
| Service | 511 | 清除異常紀錄旗標（格式在 0.2 版定義） | 5 |
| Service | 500 到 509 | 保留給協定 | |

應用使用 Subject-ID 0 到 7999、Service-ID 0 到 499。

## 二、Heartbeat

每個 Node 每秒發一次，開機後立即發一次。接收端 3 秒沒收到，就判定該 Node 離線。payload 7 B，classic CAN 單框放得下。

| 位置 | 欄位 | 長度 | 說明 |
|---|---|---|---|
| 0 | uptime | 4 | 開機後的秒數，LE，到上限後停在 0xFFFFFFFF |
| 4 | status | 1 | 見下表 |
| 5 | link_state | 1 | 見下表 |
| 6 | vendor | 1 | 應用自訂 |

status：

| 位元 | 欄位 | 值 |
|---|---|---|
| 1-0 | health | 0 正常、1 注意、2 降級、3 故障 |
| 4-2 | mode | 0 運作、1 初始化、2 維護、3 軟體更新；4 到 7 保留 |
| 5 | fault_log_unread | 1 表示有未讀的異常紀錄（`002` 第 2.2 節） |
| 6 | auth_disabled | 1 表示認證功能未啟用，例如沒有安全的金鑰儲存（`003` K-21） |
| 7 | 保留 | 0 |

link_state：這個 Node 所有 CAN Link 中最差的狀態。

| 位元 | 欄位 | 值 |
|---|---|---|
| 1-0 | can_state | 0 error active、1 error passive、2 曾經 bus off 並已恢復、3 目前 bus off |
| 7-2 | 保留 | 0 |

## 三、Service

- request 與 response 各是一筆 Transfer，CAN ID 見 `01-can-transport.md` 第 1.2 節。
- client 為每個（Service-ID、server Node-ID）維護 Transfer-ID，每送一筆 request 加 1。
- server 的 response 用同一個 Service-ID，目的 Node-ID 是 client，Transfer-ID 與 request 相同。
- client 用（Service-ID、server Node-ID、Transfer-ID）比對 response。逾時由設定檔依 Service 指定，預設 1 秒；逾時後收到的 response 丟棄。
- 一個 client 對同一個（Service-ID、server）同時進行中的 request 數量，上限由設定檔決定。
- Bridge 轉送 Service 時，以展開後模 32 等於原值的方式重新編號，response 回到原網段時 Transfer-ID 對得上（`005-prior-art.md` 第五節）。轉送細節在 P3 定義。
