# embnet-rs

用 Rust 寫的嵌入式內部網路。設計參考 Cyphal：同一條匯流排上的 Node 直接以 Subject 與 Service 互通，
不同匯流排之間由 Bridge 轉送。涵蓋 UART/RS-485、CAN/CAN FD、Ethernet/UDP，以及 Cyphal 沒有定義的
SPI、I²C、Modbus。核心同時編成 `std`（Linux）與 `no_std`（MCU，不配置記憶體）。術語見 `CONTEXT.md`。

## 一、為什麼有這個 repo

嵌入式系統的內部通訊分散在多種匯流排上，兩兩之間的轉接通常各寫一套。Cyphal 已經把
「去中心化的 pub/sub 與 service、固定型別、CAN 與 UDP 傳輸」做得很完整，但它沒有標準的跨傳輸 Bridge，
也沒有定義主從輪詢的介面。這個 repo 回答：**借用 Cyphal 的設計，補上 Bridge 與 Polled link，
能不能用一套介面串起所有內部匯流排，單跳延遲多少、體積多少，出錯時的行為是否可預期。**

## 二、已定案

| 項目 | 決定 |
|---|---|
| 與 Cyphal 的關係 | 借用設計，線格式不相容。差異列在 `docs/plans/001` 第二節 |
| Bridge 的角色 | 只在不同 Link 或 Trust zone 之間轉送，同匯流排的 Node 直接互通 |
| 主從介面 | SPI、I²C 定義 Polled link 給 Native node；Modbus 的 Legacy device 由 Adapter 翻譯 |
| 平台 | 同一份核心編成 `std` 與 `no_std`。核心不配置記憶體 |
| 第一階段介面 | UART/RS-485、CAN/CAN FD、Ethernet/UDP、SPI/I²C/Modbus |
| 參考板 | NUCLEO-F767ZI（STM32F767ZI：Cortex-M7、三個 bxCAN，沒有 CAN FD 與硬體 AES）。編譯用 `thumbv7em-none-eabihf` 加 `-C target-cpu=cortex-m7` |
| UART framing | COBS，`cobs` crate |
| 延遲目標 | 先量再定。報告分布（p50、p99、max），不用平均值 |
| 授權 | MIT。每個相依都要記授權 |
| 公開 | GitHub public repo |

## 三、硬規則

- **[HARD] 先量再寫。** Cyphal 的實作（canadensis、emcyphal、libcanard）要先量體積、配置方式與延遲
  （`docs/plans/001` 的 P0），再決定哪些程式碼可以沿用。
- **[HARD] 規格以 `docs/spec/` 為準，並附 test vector。** 至少一份 test vector 由獨立的非 Rust 程式產生，
  避免實作和規格一起錯。實作與規格不符時先改規格或開 issue。
- **[HARD] 每個里程碑要有一行可 grep 的通過行，並附 run 目錄。** 單元測試綠不算通過。
  延遲數字要附 Link 種類、速率、量測環境與樣本數；p99 至少 10,000 筆，量法依 `docs/40-rust-embedded/04`。
- **[HARD] 一律在容器裡建置與測試**（`--rm`、限 CPU 與記憶體、預設 `--network none`）。不用主機的 rustup 或 pip。
- **[HARD] 這是 public repo。** 不帶入任何客戶、車型、場域或主機名稱。不提交金鑰、憑證或 token；
  測試用金鑰放在 `tests/`，檔名標 `test-only`。
- **[HARD] `git config user.email` 是 `wicanr2@gmail.com`（repo-local）。** commit message 用繁體中文。
- **[HARD] 核心 crate `#![forbid(unsafe_code)]`，解析路徑不得 panic。** 需要 `unsafe` 的驅動、DMA 與 `#[link_section]`
  只放在 Link crate 或韌體 crate，每處都寫明不變式。不使用 `static mut`（見 `docs/40-rust-embedded/01`）。
- **[HARD] 新增 Link、Subject 或線格式欄位時，對照 `docs/plans/002` 的故障表與威脅表。**

## 四、非目標

- 不與 Cyphal、ROS 2 或 DDS 互通。
- 第一階段不做無線（Wi-Fi、BLE、Zigbee、Thread）與 mesh 多跳路由。
- 不提供功能安全認證。設計不排除日後認證，但 Bridge 預設不擔任安全功能（見 `docs/plans/002`）。
- 不在 repo 裡放應用層訊息型別。

## 五、判準

| 判準 | 怎麼量 |
|---|---|
| 涵蓋 | 四類介面各有一個 Node 經 Bridge 與另一類介面上的 Node 完成 pub/sub 與 service 往返 |
| 延遲 | 同匯流排直連與經 Bridge 單跳的分布，拆成 Wire time、Bridge 處理時間、排隊、輪詢等待 |
| 體積 | `thumbv7em-none-eabihf` 上 Node 程式庫與 Bridge 各自的 `.text` + `.rodata` 與靜態 RAM |
| 隔離 | 一條 Link 滿載或故障時，其他 Link 的 p99 變化 |
| 負載 | Bridge 出口使用率從 10% 到 100% 的 p99 與丟棄數，找出轉折點（`docs/plans/003` 第一節） |
| 安全性 | 畸形輸入不 panic、不越界，fuzz 附覆蓋率；故障注入下的行為符合 `docs/plans/002` |
