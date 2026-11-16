# embnet-rs

用 Rust 寫的嵌入式內部網路。一台 Router 串接 UART/RS-485、CAN/CAN FD、Ethernet/UDP、
SPI/I²C/Modbus 上的裝置，對應用端提供同一套 topic pub/sub 與 service 介面，並把延遲壓低。
架構借用 XRCE 的 client-agent 概念，線格式自訂，不與 ROS 2 互通。

## 一、為什麼有這個 repo

嵌入式系統的內部通訊分散在多種匯流排上。每種匯流排的定址、框架、MTU、存取模型
（多主或主從）與可靠度都不同，兩兩之間的轉接通常各寫一套。這個 repo 回答：
**能不能用一個 Router 加上一套統一介面串起這些匯流排，單跳延遲多少，體積多少，
出錯時的行為是否可預期。**

XRCE 留下來的是架構：資源有限的裝置只保留最少狀態，session、路由表與 topic 註冊交給
Router 保管。DDS、CDR XTypes 與 XRCE 線格式都不沿用。

## 二、已定案

| 項目 | 決定 |
|---|---|
| Router 平台 | 同一份核心同時編成 `std`（Linux 閘道）與 `no_std`（MCU）。核心不配置記憶體 |
| 互通 | 不與 ROS 2 或標準 XRCE agent 互通。線格式自訂，規格寫在 `docs/spec/` |
| 第一階段介面 | UART/RS-485、CAN/CAN FD、Ethernet/UDP、SPI/I²C/Modbus |
| 延遲目標 | 先量再定。報告分布（p50、p99、max），不用平均值 |
| 授權 | MIT。每個相依都要記授權 |
| 公開 | GitHub public repo |

## 三、硬規則

- **[HARD] 先量再寫。** Zenoh（zenoh-pico）與 Cyphal（canadensis、emcyphal）要先做涵蓋率、延遲與體積的對照
  （`docs/plans/001` 的 P0），再決定採用、延伸或自寫。
- **[HARD] 每個里程碑要有一行可 grep 的通過行，並附 run 目錄。** 單元測試綠不算通過。
  延遲數字要附鏈路種類、速率與樣本數。
- **[HARD] 一律在容器裡建置與測試**（`--rm`、限 CPU 與記憶體、預設 `--network none`）。不用主機的 rustup 或 pip。
- **[HARD] 這是 public repo。** 不帶入任何客戶、車型、場域或主機名稱。不提交金鑰、憑證或 token；
  測試用金鑰放在 `tests/` 並在檔名標 `test-only`。
- **[HARD] `git config user.email` 是 `wicanr2@gmail.com`（repo-local）。** commit message 用繁體中文。
- **[HARD] 核心 crate `#![forbid(unsafe_code)]`，解析路徑不得 panic。** 需要 `unsafe` 的硬體驅動只放在鏈路 crate，
  每處都寫明不變式。
- **[HARD] safety 與 security 的要求見 `docs/plans/002`。** 新增鏈路、topic 或線格式欄位時，要對照該文件的危害表與威脅表。
- **[HARD] 規格以 `docs/spec/` 為準。** 實作與規格不符時先改規格或開 issue，不讓實作行為變成事實標準。

## 四、非目標

- 不做 DDS、RTPS，也不與 ROS 2 互通。
- 第一階段不做無線（Wi-Fi、BLE、Zigbee、Thread）與 mesh 多跳路由。
- 不提供功能安全認證。設計不排除日後認證，但 Router 預設不是安全功能（見 `docs/plans/002`）。
- 不在 repo 裡放應用層訊息型別。

## 五、判準

| 判準 | 怎麼量 |
|---|---|
| 涵蓋 | 四類介面各有一個裝置經 Router 與另一類介面上的裝置完成 pub/sub 與 service 往返 |
| 延遲 | 單跳延遲分布，拆成 wire time、Router 處理時間、排隊時間、輪詢等待 |
| 體積 | `thumbv7em-none-eabihf` 上 Router 與 client 各自的 `.text` + `.rodata` 與靜態 RAM |
| 隔離 | 一條鏈路滿載或故障時，其他鏈路的 p99 變化 |
| 安全性 | 畸形封包不 panic、不越界，fuzz 附覆蓋率數字；故障注入下的行為符合 `docs/plans/002` |
