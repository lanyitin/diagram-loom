# maxGraph：可丟棄的原型

> **這個資料夾可以整個刪掉。** 要留下來的是
> [`docs/maxgraph-findings.md`](../../../docs/maxgraph-findings.md)。

## 它在回答什麼問題

[`docs/canvas-engine.md`](../../../docs/canvas-engine.md) 列了四個問題。
換掉 draw.io 不是換一個畫布，是**從「嵌一個做好的編輯器」變成「自己做一個
編輯器」**，所以在寫任何正式程式碼之前要先知道這四件事：

| | 問題 | 誰答得了 |
| --- | --- | --- |
| 1 | elkjs 自己接，巢狀排得好嗎 | `pnpm probe`（座標是純計算） |
| 2 | 少了 draw.io 的編輯器，到底有多痛 | **人**（`pnpm app` 然後去用） |
| 3 | 點形狀 → 標註它代表誰，做起來多快 | `pnpm click`＋人 |
| 4 | `stencils/*.xml` 搬不搬得動 | `pnpm probe`＋`pnpm click` |

## 跑法

```sh
pnpm install
pnpm probe      # 不開視窗：排版品質、規模、形狀庫（23 項）
pnpm app        # 開一個真的畫面，自己去點
pnpm click      # 另開終端機，用真的 Chrome 點下去（11 項）
```

離開碼：`0` 全過、`1` 有項目失敗。

`pnpm click` 用的是系統上已經裝好的 Chrome（`puppeteer-core`），
不另外下載一份瀏覽器。路徑寫死在 `src/click-probe.mjs` 開頭。

形狀庫的部分需要 `../../vendor/drawio/stencils/`，那包不進版控——
沒有的話那幾項會紅燈，其他照跑。

## 素材為什麼跟上次一樣

`src/fixture.mjs` 是 [`docs/nested-layout.md`](../../../docs/nested-layout.md)
那一輪**完全一樣**的圖（三層巢狀、兄弟容器、跨容器的邊、一個 hub），
四項客觀檢查也照抄。

一樣才比得出來——換一份素材的話，「elkjs 排得比 draw.io 好還是壞」
這個問題就沒有答案了，差異可能只是素材不同。

## 兩張圖

`out/screenshot.png` 是正常的樣子。
`out/no-bends.png` 是**故意不把 ELK 的轉彎點寫回去**的樣子——線直接穿過
三層容器。那是「自己接要付多少」最具體的一張圖，所以留著。

（`window.__noBends = true` 就會切成那個模式。）

## 為什麼不是 Tauri

階段 0 的原型要驗的是「Tauri 的 CSP 與 iframe」，所以非得開 Tauri 不可。
這一輪要驗的全是**瀏覽器裡的事**：排版算得對不對、點得到形狀嗎、
stencil 畫不畫得出來。用 vite 就好，省掉一次 Rust 編譯。
