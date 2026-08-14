/**
 * 螢光筆：把不相關的東西調暗。
 *
 * 這是 `diagram.ts` 的 `dim` / `spotlight` / `undim` 同一套規則，
 * substrate 從 XML 字串換成 cells。**規則一個字都沒變。**
 *
 * # 這支螢光筆只有一個顏色
 *
 * **只動 `opacity`，其他樣式一個字都不改。** style 裡還住著使用者自己調的
 * 顏色、框線、字體——界線講死了，我們才可以放心反覆重塗，不必記
 * 「它原本是多少」（見 `docs/drawio-integration.md`）。
 *
 * # 不刪任何東西
 *
 * 調暗不是隱藏。隱藏等於把課本剪掉，剪過的課本永遠回答不了「有沒有漏字」；
 * 調暗之後**圖上的形狀一個都沒少**，所以對帳完全不受篩選影響。
 */

import type { Cell, GraphDataModel } from '@maxgraph/core'

import { allCells, attribute } from './binding'

/** 調暗的透明度。25% 淡到不會搶，但還看得出那裡有東西。 */
const DIM = 25

/**
 * 把不在 `lit` 裡的形狀與線調暗。
 *
 * 只碰**我們畫的或綁著模型的**（有 `loomKind` 或 `loomId`）。
 * 使用者自己畫的裝飾不碰：那不是我們的東西。
 */
export function dim(model: GraphDataModel, lit: Set<string>): void {
  paint(model, (key) => lit.has(key), false)
}

/**
 * 只讓一個形狀亮著，其他全部調暗——**包括使用者自己畫的**。
 *
 * 標註清單用它回答「這一項到底是圖上哪一個框」。清單有 47 個名字時，
 * 光靠名字對不出來，而對不出來的人就會亂指——那比沒有這個功能更糟。
 */
export function spotlight(model: GraphDataModel, cellId: string): void {
  paint(model, (key) => key === cellId, true)
}

/**
 * 把我們塗上去的螢光筆擦掉。
 *
 * 少了它，關掉篩選之後圖還是暗的——`opacity` 已經寫進 cell 的樣式裡了。
 */
export function undim(model: GraphDataModel): void {
  paint(model, () => true, true)
}

/**
 * 螢光筆的本體。
 *
 * `unbound` 決定碰不碰使用者自己畫的形狀：篩選只講模型的事，所以不碰；
 * 標註要指的正是那些形狀，所以碰。
 */
function paint(model: GraphDataModel, isLit: (key: string) => boolean, unbound: boolean): void {
  model.beginUpdate()
  try {
    for (const cell of allCells(model)) {
      const ours = attribute(cell, 'loomKind') != null || attribute(cell, 'loomId') != null
      if (!ours && !unbound) continue

      // 線的 `loomId` 是連線 id，形狀的是元素 id；人沒有 `loomId`，
      // 用 cell id 認（`Highlight.shapes` 裡放的就是人的 id）。
      const key = attribute(cell, 'loomId') ?? cell.id ?? ''
      model.setStyle(cell, withOpacity(cell.getStyle(), isLit(key)))
    }
  } finally {
    model.endUpdate()
  }
}

/**
 * 換掉樣式裡的 `opacity`，其餘原樣保留。
 *
 * 亮的就把 `opacity` **拿掉**，而不是設成 100——留著的話使用者自己調的
 * 半透明會被我們永久蓋掉。
 */
function withOpacity(style: unknown, isLit: boolean): Record<string, unknown> {
  const next = { ...(style as Record<string, unknown> | null ?? {}) }
  if (isLit) delete next.opacity
  else next.opacity = DIM
  return next
}

/** 圖上目前有沒有被塗過。塗過才需要擦——沒塗過就擦會洗掉使用者自己調的半透明。 */
export function smeared(model: GraphDataModel): boolean {
  return allCells(model).some((cell) => (cell.getStyle() as Record<string, unknown>)?.opacity != null)
}

/** 這個 cell 現在是不是暗的。給測試與畫面判斷用。 */
export function opacityOf(cell: Cell): number | undefined {
  return (cell.getStyle() as Record<string, unknown>)?.opacity as number | undefined
}
