# 04 UART 與 UDP

版本 0.2。UART 與 UDP 都是「一次送一整筆」的 Link：一筆 Transfer 就是一個 datagram，不分框。兩者共用同一個 datagram 格式，UART 另外用 COBS 分界。

## 一、datagram 格式

| 位置 | 欄位 | 長度 | 說明 |
|---|---|---|---|
| 0 | version | 1 | 0 |
| 1 | flags | 1 | 見下表 |
| 2 | port | 2 | Subject-ID 或 Service-ID，LE |
| 4 | source | 1 | 來源 Node-ID，0 到 127 |
| 5 | destination | 1 | 目的 Node-ID；Subject 時填 0xFF |
| 6 | transfer_id | 1 | 0 到 255 |
| 7 | 保留 | 1 | 0 |
| 8 起 | payload | n | 整筆 Transfer 的 payload |
| 最後 4 B | CRC-32C | 4 | 算過位置 0 到 payload 結尾，big-endian |

flags：

| 位元 | 欄位 | 說明 |
|---|---|---|
| 2-0 | priority | 0 到 7 |
| 3 | kind | 0 Subject，1 Service |
| 4 | request | Service 時 1 是 request；Subject 時填 0 |
| 7-5 | 保留 | 0 |

規則：

- version 不是 0、保留欄位不是 0、Subject 的 destination 不是 0xFF、Node-ID 超出範圍、長度小於 12 或 CRC 不符：整個 datagram 丟棄並計數。
- Transfer-ID 每個 session 各自加 1，模 256。session 的定義、去重複與逾時同 `01-can-transport.md` 第四、六節。
- Service 的 response 沿用 request 的 Transfer-ID。
- payload 的最大長度由設定檔依型別決定，不得超過 Link 的上限（UDP 為 1472 − 12 = 1460 B；UART 由設定檔指定）。

CRC-32C（CRC-32/ISCSI）的參數：poly 0x1EDC6F41、init 0xFFFFFFFF、refin 與 refout 是、xorout 0xFFFFFFFF、check 0xE3069283。
它與 E2E 用的 CRC-32/AUTOSAR 是不同的多項式（`README.md`）。

## 二、UART：COBS

- 一個 datagram 以 COBS 編碼後，接一個 0x00 當分界。編碼後的資料不含 0x00。
- 資料以剛好 254 個非零位元組的區塊結束時（code byte 為 0xFF），編碼端**不再**附加 code byte 0x01。
  另一種常見寫法會多附加一個 0x01；兩種寫法解碼結果相同，解碼端必須都接受。
- 接收端收到 0x00 就把之前累積的位元組當一個 COBS 區塊解碼；解碼失敗、長度超過上限或 datagram 檢查失敗就丟棄，從下一個位元組重新開始。
- 連續的 0x00（空區塊）直接忽略。發送端可以在 datagram 之前先送一個 0x00，讓接收端從線路雜訊中對齊。
- COBS 的最壞開銷是每 254 B 多 1 B，再加分界 1 B（`docs/20-protocol/03` 第八節）。
- RS-485 半雙工時，發送端要等 transmission complete 才能放開線路驅動器（`003` K-09）。

## 三、UDP

- 一個 UDP datagram 帶一個 embnet datagram，不分段。
- 位址與埠由設定檔指定，預設埠 46000。單播或群播由設定檔決定。
- IP 與 UDP 本身的 checksum 不能取代第一節的 CRC-32C：IPv4 的 UDP checksum 可以關閉。
