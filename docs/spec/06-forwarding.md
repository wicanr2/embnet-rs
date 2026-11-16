# 06 Bridge 的轉送

版本 0.2。Bridge 只轉送設定檔轉送表列出的（Subject 或 Service、來源 Node-ID、入口 Link、出口 Link），其餘丟棄並計數。檢查順序見 `docs/30-safety-security/03` 第四節。

## 一、保留與重做的欄位

| 層 | 欄位 | Bridge 的處理 |
|---|---|---|
| 邏輯 | Priority、Subject-ID 或 Service-ID、來源 Node-ID、目的 Node-ID | 原樣保留 |
| E2E | `seq_low`、應用資料、CRC 或 tag | 原樣保留，不驗證；跨 Trust zone 且設定檔要求時才驗證 MAC |
| Link | Transfer-ID、Tail byte、多框 CRC、datagram 標頭與 CRC-32C、COBS | 依出口 Link 重做 |

## 二、轉送方式

- **格式相同的 CAN 到 CAN（MTU 也相同）：** 逐框轉送，不重組，Transfer-ID 不變。入口收到的框依序放進出口佇列。
- **其他組合：** 先重組成完整的 Transfer、通過 Link 層的檢查，再依出口格式重新分框或包成 datagram。

## 三、Transfer-ID 的對應

不同 Link 的 Transfer-ID 模數不同：CAN 是 32，datagram 是 256。Bridge 為轉送表中的每個 session 保存一個 32 位元的展開值 U：

1. 入口收到一筆 Transfer，Transfer-ID 為 t，入口的模數為 m。
2. d = (t − (U mod m)) mod m；U = U + d（模 2³²）。第一次見到這個 session 時，U = t。
3. 出口的 Transfer-ID = U mod 出口的模數。

因為 m 整除 2³²，而且兩種模數都是 2 的冪，U mod m 永遠等於入口原本的 t。Service 的 response 從出口回到入口時，用同一個表反向計算，Transfer-ID 會與原 request 一致（`005-prior-art.md` 第五節）。

表的大小在建置期由轉送表決定，不為未列出的 session 配置空間。

## 四、去重複與丟棄

- 入口照 `01-can-transport.md` 第六節去重複；出口不再去重複。
- 出口佇列滿時，依 Subject 的丟棄策略：狀態類丟最舊的，指令類丟最新的；逾期的 Transfer 在出隊時丟棄（`docs/20-protocol/04` 第六節）。
- 丟棄、未列入轉送表、Node-ID 超出出口範圍，都計數並隨診斷 Subject 發佈。協定保留的 Subject（8000 以上，例如 Heartbeat）沒有轉送時另外計數，不算進「未列入轉送表」，避免正常的本地流量灌高 security 事件。

## 五、E2E Sync 跟著 `protected` Subject 轉送

E2E Sync（Subject-ID 8189，`02-e2e.md` 第五節）帶著發佈者所有 `protected` Subject 的下一個序號。接收端沒有收到 Sync 就無法同步，所有結果都是 NOSYNC。
所以轉送表裡每一條 `protected` Subject 的規則，都自動附帶一條同一發佈者、同一入口與出口的 E2E Sync 規則；同一發佈者、同一路徑只附帶一條。
這條規則的限流取每秒 1 筆、突發 2 筆。設定檔不必另外列出，`embnet-config` 產生轉送表時加入。
