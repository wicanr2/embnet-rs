# embnet 規格

版本：0.1（草案）。實作與規格不符時，先改規格或開 issue（`CLAUDE.md`）。每個欄位都要有 test vector，放在 `vectors/`，
由 `tools/vectors/gen.py` 產生。這支 Python 程式獨立撰寫，不共用 Rust 的程式碼（`003` K-10）。

| 文件 | 內容 | 狀態 |
|---|---|---|
| [01-can-transport.md](01-can-transport.md) | CAN ID、Tail byte、分框、多框 CRC、重組、Transfer-ID | 0.1 |
| [02-e2e.md](02-e2e.md) | E2E 保護：Data ID、序號、CRC、接收端判定 | 0.1 |
| [03-node-services.md](03-node-services.md) | Heartbeat、Service 的 request 與 response、保留的 ID | 0.1 |
| UART（COBS）、UDP | Link 層 framing | P3 |
| MAC 與 FV | `002` 第四節的方案 A、B | P6 |
| 設定檔與建置期檢查 | `embnet-config` 的輸入格式 | P1 後半 |

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

參數與 check 值依 RevEng CRC catalogue（`docs/20-protocol/03` 第五節）。兩層用不同的多項式，避免同一種錯誤樣式同時騙過兩層。

## 名詞

術語依 `CONTEXT.md`。規格裡的「必須」是強制要求，「應」是建議，「可以」是選項。
