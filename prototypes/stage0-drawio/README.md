# 階段 0：可丟棄的原型

> **這個資料夾可以整個刪掉。** 要留下來的是 [`docs/stage0-findings.md`](../../../docs/stage0-findings.md)。

## 它在回答什麼問題

在寫任何前端之前，先確認 **Tauri 內嵌 draw.io 走不走得通**。
如果走不通，`docs/drawio-integration.md` 整份都要重寫。

風險依序是：

1. Tauri 的 CSP 會不會擋掉本地 iframe
2. draw.io 的嵌入協定（`postMessage`）在 Tauri 的 webview 裡跑不跑得動
3. **`loomId` 自訂屬性能不能活過一次真正的模型異動**——這是對帳機制的前提

## 跑法

```sh
pnpm install
pnpm probe          # 打真的 draw.io
pnpm probe:stub     # 打對照組
```

會開一個視窗、自己跑完一輪、把結果印到終端機、然後結束。
離開碼：`0` 全過、`1` 有項目失敗、`2` 探針根本沒回報。

需要 `../../vendor/drawio/`（從 [draw.war](https://github.com/jgraph/drawio/releases) 解壓）。

## 對照組的用途

`web/stub/` 是一支只實作嵌入協定、不畫圖的假 draw.io。
它跟真的 draw.io **走完全一樣的管路**（同一個 `drawio://` 協定，只有 Rust 那端的根目錄不同）：

| 真的 draw.io | 對照組 | 結論 |
| --- | --- | --- |
| 掛 | 掛 | 問題在 Tauri 的 CSP 或協定註冊 |
| 掛 | 過 | 問題在 draw.io 本身 |
| 過 | — | 沒事 |

沒有對照組的話，一個紅燈只會得到「不知道為什麼」。

## 為什麼 draw.io 不放進 `frontendDist`

解壓後 **152 MB**。`frontendDist` 的內容會在編譯期嵌進執行檔，
152 MB 進去編譯會慢到不能用。所以註冊了一個 `drawio://` 自訂協定從磁碟供應。

副作用是 draw.io 落在**自己的 origin** 上，跟主視窗跨來源——
但嵌入協定本來就是為跨來源設計的，而且這樣主視窗的 CSP 不會意外綁住 draw.io。

## 為什麼有自己的 `[workspace]`

`src-tauri/Cargo.toml` 開頭那個空的 `[workspace]` 不是裝飾。
它讓根目錄的 `cargo test --workspace` 完全看不到這個 crate，
`tauri` 的相依樹也爬不進 `loom-core`——就是那條 ⭐ 紀律的執行機制。
