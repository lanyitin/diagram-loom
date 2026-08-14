/**
 * 綁定：圖上的形狀代表哪一個模型元素。
 *
 * 這是 `diagram.ts` 那幾支（`boundShapes` / `unboundShapes` / `bind` / `unbind`）
 * 的同一套規則，只是substrate 從 XML 字串換成 maxGraph 的 cells。
 * **規則一個字都沒變**——對帳吃的就是這些規則。
 *
 * # 綁定住在 cell 的 value 上
 *
 * value 是一個 XML 元素（`<object label loomId loomKind>`），跟 draw.io 的
 * 自訂屬性是同一件事，所以存檔時原封不動寫得回去（見 `mxml.ts`）。
 *
 * ⚠️ **綁定不能記在畫面的變數裡。** 記在變數裡的話，畫面一副已經指定好的
 * 樣子，而圖上什麼都沒有、存出去也什麼都沒有、對帳看不見——那是這個工具
 * 最不能出的那種錯（原型踩過，見 `docs/editor-widgets.md` 的坑 9）。
 */

import type { Cell, GraphDataModel } from '@maxgraph/core'

import type { UnboundShape } from '../model'

/** 圖上所有形狀與線。root 底下第一層是圖層，不算。 */
export function allCells(model: GraphDataModel): Cell[] {
  const out: Cell[] = []
  const walk = (cell: Cell) => {
    for (const child of cell.children ?? []) {
      if (child.isVertex() || child.isEdge()) out.push(child)
      walk(child)
    }
  }
  for (const layer of model.getRoot()?.children ?? []) walk(layer)
  return out
}

/** 讀一個自訂屬性。值還是字串（沒有屬性）時回 `null`。 */
export function attribute(cell: Cell, name: string): string | null {
  const value = cell.getValue() as unknown
  return isElement(value) ? value.getAttribute(name) : null
}

/**
 * 圖上綁著的 `[{id, label}]`——對帳的另一半。
 *
 * ⚠️ **依 id 去重**：一條萬用字元連線在圖上是 N×M 條線、共用一個 `loomId`，
 * 而對帳那邊的模型側每條連線只有一個元素。不去重的話同一條連線會被數很多次，
 * 衝突判斷跟著錯。
 */
export function boundShapes(model: GraphDataModel): { id: string; label: string }[] {
  const out = new Map<string, { id: string; label: string }>()
  for (const cell of allCells(model)) {
    const id = attribute(cell, 'loomId')
    if (!id || out.has(id)) continue
    // 標籤裡有換行（名字＋副標），對帳只要第一行。
    out.set(id, { id, label: (attribute(cell, 'label') ?? '').split('\n')[0]!.trim() })
  }
  return [...out.values()]
}

/**
 * 圖上**還沒指定**代表誰的形狀。
 *
 * 這是 [`boundShapes`] 的反面，而反面才是使用者上傳自己畫的圖時看到的東西。
 *
 * - **沒有文字的也要列**：空白方框多半是裝飾，但自己決定「這個不用管」，
 *   使用者就永遠不知道有這回事——這個工具的命是怕漏。
 * - **我們自己畫的不算**：帶 `loomKind` 的是我們產的（例如「人」，它刻意
 *   沒有 `loomId`）。那些不是「還沒指定」，是「本來就不該指定」。
 */
export function unboundShapes(model: GraphDataModel): UnboundShape[] {
  return allCells(model)
    .filter((cell) => !attribute(cell, 'loomId') && !attribute(cell, 'loomKind'))
    .map((cell) => ({
      cell: cell.id ?? '',
      label: plain(labelOf(cell)),
      edge: cell.isEdge(),
    }))
}

/** 圖上的文字：值是元素就讀 `label`，是字串就用它自己。 */
function labelOf(cell: Cell): string {
  const value = cell.getValue() as unknown
  if (isElement(value)) return value.getAttribute('label') ?? ''
  return value == null ? '' : String(value)
}

/**
 * 把 draw.io 的標籤變成一行純文字。
 *
 * 標籤可以是 HTML（`html=1` 是預設），所以使用者畫的圖上常常是
 * `<b>Apache</b><br>叢集`。直接拿去比對名字會一個都對不上。
 */
function plain(label: string): string {
  return label
    .replace(/<br\s*\/?>|<\/(div|p|li)>/gi, '\n')
    .replace(/<[^>]*>/g, '')
    .replace(/&nbsp;/g, ' ')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    // `&amp;` 最後才解，否則 `&amp;lt;` 會被解兩次變成 `<`。
    .replace(/&amp;/g, '&')
    .split('\n')[0]!
    .trim()
}

/**
 * 指定一個形狀代表誰。傳 `null` 就是取消指定。
 *
 * 值還是字串的形狀（使用者自己畫的）會先換成 XML 元素——draw.io 按
 * 「編輯資料」時做的也是這件事。
 *
 * ⚠️ 換值之前畫面要先教會 maxGraph「值可能是元素」，否則標籤會變成
 * `object`（見 `useElementValues`）。
 */
export function bind(model: GraphDataModel, cellId: string, loomId: string | null): boolean {
  const cell = model.getCell(cellId)
  if (!cell) return false

  const next = asElement(cell)
  if (loomId == null) next.removeAttribute('loomId')
  else next.setAttribute('loomId', loomId)

  model.beginUpdate()
  try {
    model.setValue(cell, next)
  } finally {
    model.endUpdate()
  }
  return true
}

/** 把 cell 的值換成 XML 元素（如果它還是字串）。回傳那個元素的複本。 */
function asElement(cell: Cell): Element {
  const value = cell.getValue() as unknown
  if (isElement(value)) return value.cloneNode(true) as Element

  const doc = new DOMParser().parseFromString('<object/>', 'text/xml')
  const el = doc.documentElement
  el.setAttribute('label', value == null ? '' : String(value))
  return el
}

function isElement(value: unknown): value is Element {
  return Boolean(value) && typeof value === 'object' && 'getAttribute' in (value as object)
}
