# embnet 規格

版本：0.2（草案）。實作與規格不符時，先改規格或開 issue（`CLAUDE.md`）。每個欄位都要有 test vector，放在 `vectors/`，
由 `tools/vectors/gen.py` 產生。這支 Python 程式獨立撰寫，不共用 Rust 的程式碼（`003` K-10）。

| 文件 | 內容 | 狀態 |
|---|---|---|
| [01-can-transport.md](01-can-transport.md) | CAN ID、Tail byte、分框、多框 CRC、重組、Transfer-ID | 0.1 |
| [02-e2e.md](02-e2e.md) | E2E 保護：Data ID、序號、CRC、接收端判定 | 0.1 |
| [03-node-services.md](03-node-services.md) | Heartbeat、Service 的 request 與 response、保留的 ID | 0.1 |
| [04-datagram-links.md](04-datagram-links.md) | UART（COBS）與 UDP 的 datagram 格式 | 0.2 |
| [05-mac.md](05-mac.md) | 截短 AES-128-CMAC、失敗預算、新鮮度與資料年齡 | 0.2 |
| [06-forwarding.md](06-forwarding.md) | Bridge 保留與重做的欄位、Transfer-ID 的對應 | 0.2 |
| [07-config.md](07-config.md) | 設定檔、Type hash、建置期檢查 | 0.2 |

## 共通約定

- **位元編號：** 位元 0 是最低位。CAN ID 的位元 28 最先上線。
- **位元組順序：** 多位元組的整數一律 little-endian，**CRC 除外**：CRC 一律 big-endian（最高位元組在前）附在被保護的資料之後。
- **保留位元：** 發送端填 0。接收端遇到不是 0 的保留位元，丟棄整個框，並計數。
- **長度：** 以位元組計。MTU 指一個 Frame 可帶的資料量，classic CAN 是 8，CAN FD 是 64。

## CRC

| 名稱 | 用途 | poly | init | refin/refout | xorout | check（"123456789"） |
|---|---|---|---|---|---|---|
| CRC-16/IBM-3740（也稱 CRC-16/CCITT-FALSE） | 多框 Transfer 的 Link 層 CRC | 0x1021 | 0xFFFF | 否 | 0x0000 | 0x29B1 |
| CRC-32/AUTOSAR | E2E CRC | 0xF4ACFB13 | 0xFFFFFFFF | 是 | 0xFFFFFFFF | 0x1697D06A |
| CRC-32C（CRC-32/ISCSI） | UART 與 UDP 的 datagram CRC | 0x1EDC6F41 | 0xFFFFFFFF | 是 | 0xFFFFFFFF | 0xE3069283 |
| CRC-64/XZ | Type hash | 0x42F0E1EBA9EA3693 | 全 1 | 是 | 全 1 | 0x995DC9BBDF1939FA |

參數與 check 值依 RevEng CRC catalogue（`docs/20-protocol/03` 第五節）。Link 層與 E2E 層用不同的多項式，避免同一種錯誤樣式同時騙過兩層。

## 名詞

術語依 `CONTEXT.md`。規格裡的「必須」是強制要求，「應」是建議，「可以」是選項。
