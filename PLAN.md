# 知識庫與開工的進度

這個 repo 分兩階段：先建基礎知識庫，把設計會用到的原理一篇一篇寫清楚；再依 `docs/plans/001` 的里程碑開工。
知識庫每篇都從要解決的根本問題推起，數學與流程配 SVG 圖，引用原始來源；查不到來源的細節標「待查證」。

## 每輪做法

1. 派研究 agent 查原始來源，回傳素材與實際查過的 URL。
2. 主迴圈撰寫文章，並畫 SVG、轉 PNG 自我檢查。
3. 更新 `README.md` 的索引與 `CONTEXT.md` 的術語。
4. commit 並 push。

全部寫完後做一次審查：專家角度查技術正確性與邊界，學生角度查有軟體背景但沒做過嵌入式的人看不看得懂。

## 第一階段：基礎知識庫

| 輪 | 檔案 | 主題 | 狀態 |
|---|---|---|---|
| 0 | `docs/plans/001` 到 `003` | 計畫、safety 與 security、風險評估 | 完成 |
| 1 | `docs/10-buses/01-can-arbitration.md` | CAN 的仲裁、填充位元、錯誤處理與 bus-off；bxCAN 與 FDCAN 的差異 | 完成 |
| 2 | `docs/10-buses/02-wire-time.md` | Wire time 怎麼推算，延遲由哪幾段組成 | 完成 |
| 3 | `docs/10-buses/03-polled-links.md` | Polled link 怎麼設計：Mailbox、中斷線、輪詢排程、RS-485 多主仲裁、Modbus 暫存器對應 | 完成 |
| 4 | `docs/20-protocol/01-cyphal-design.md` | Cyphal 的設計：去中心化、Subject 與 Service、Transfer | 未開始 |
| 5 | `docs/20-protocol/02-fragmentation.md` | 分片、重組與 Tail byte | 未開始 |
| 6 | `docs/20-protocol/03-crc-and-framing.md` | CRC 從多項式除法推起；HDLC 與 COBS 的 framing | 未開始 |
| 7 | `docs/20-protocol/04-bridge-queueing.md` | 集中式與去中心化；Bridge 的轉送語意與排隊 | 未開始 |
| 8 | `docs/30-safety-security/01-fault-model-e2e.md` | 通訊故障模型與 E2E 保護 | 未開始 |
| 9 | `docs/30-safety-security/02-mac-freshness.md` | MAC、FV 與重放；截短 MAC 的強度 | 未開始 |
| 10 | `docs/30-safety-security/03-trust-zones.md` | Trust zone、Bridge 當防火牆、STRIDE | 未開始 |
| 11 | `docs/40-rust-embedded/01-no-alloc.md` | `no_std` 與不配置記憶體：緩衝由誰持有 | 未開始 |
| 12 | `docs/40-rust-embedded/02-sans-io.md` | sans-IO：把協定和 I/O、時鐘分開 | 未開始 |
| 13 | `docs/40-rust-embedded/03-testing.md` | fuzz、性質測試與 Miri | 未開始 |
| 14 | `docs/40-rust-embedded/04-latency-measurement.md` | 延遲分布怎麼量：p99、DWT 週期計數器、時戳 | 未開始 |
| 15 | 全部 | 專家與學生審查，套用修正 | 未開始 |

## 第二階段：開工

知識庫完成後，依 `docs/plans/001` 第七節從 P0 開始，並先處理 `001` 第十節與 `002` 第七節的待審查決定。
開工前另寫一份 P0 的執行細節：容器映像與版本、量測腳本、通過行格式。
