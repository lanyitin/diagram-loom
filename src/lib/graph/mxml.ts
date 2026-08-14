/**
 * maxGraph 的模型 ↔ draw.io 的 `.drawio` XML。
 *
 * # 為什麼兩邊都自己寫
 *
 * **寫**不能用 maxGraph：它 0.24 匯出的是**自己的格式**，不是 draw.io 的。
 *
 * ```text
 * draw.io    <mxGraphModel> <mxCell style="rounded=0;fillColor=none;"> <mxGeometry x="40">
 * maxGraph   <GraphDataModel> <Cell> <Object rounded="0" as="style"/>  <Geometry _x="40">
 * ```
 *
 * 用它存檔，檔案 **draw.io 打不開**，而且跟 `reconcile` 讀的東西對不上——
 * 對帳靠的就是 `<object loomId=...>` 這個形狀。
 *
 * **讀**本來想用 maxGraph 的 `ModelXmlSerializer`（在真的瀏覽器裡它讀得很好），
 * 但它在 **happy-dom 下讀不到 style 與幾何**——實測連最單純的一份 XML 都是
 * 空的 style 與 0 座標。而 happy-dom 正是這個專案快速測試層的環境
 * （見 `docs/development-setup.md`）。
 *
 * 讀檔是**最該被測的一條路**：每一張存過的圖、每一份使用者上傳的原稿都走它，
 * 而它壞掉的樣子是「打開一片空白」或「座標全跑掉」。那種東西不能只靠
 * 開 App 用眼睛看。
 *
 * 所以兩邊都自己寫，代價是**我們只認得自己需要的那個子集**：
 * `<mxCell>` / `<object>` / `<mxGeometry>` / `<Array as="points">`。
 * 真實 draw.io 檔案的長尾（壓縮過的內容、圖片、自訂形狀）解析得出結構，
 * 但畫不畫得出來是另一回事——那屬於形狀庫的問題（見 `maxgraph-findings.md`）。
 *
 * # 這個檔案是**轉接**不是規則
 *
 * 照 CLAUDE.md 的分界：XML 長什麼樣是 draw.io 那邊的細節，判斷都在 Rust。
 * 這裡只把已經算好的東西換一種寫法。
 */

import { Cell, Geometry, type GraphDataModel, Point } from '@maxgraph/core'

/** draw.io 的形狀有兩種包裝：光禿禿的 `<mxCell>`，或被 `<object>` 包起來。 */
const WRAPPERS = new Set(['object', 'UserObject'])

/**
 * 把一份 `.drawio` 讀進 maxGraph 的模型。
 *
 * 吃得下整份 `<mxfile>`，也吃得下光禿禿的 `<mxGraphModel>`——使用者上傳的
 * 檔案兩種都有，而少判斷這一下的代價是「打開是一片空白」。
 */
export function readXml(model: GraphDataModel, xml: string): void {
  const doc = new DOMParser().parseFromString(xml, 'text/xml')
  const rootEl = doc.querySelector('root')
  if (!rootEl) throw new Error('這不是一份 .drawio：找不到 <root>')

  const cells = new Map<string, Cell>()
  /** 第二趟才接父子與兩端：draw.io 的檔案裡，線可以寫在它連的形狀前面。 */
  const links: { cell: Cell; parent: string | null; source: string | null; target: string | null }[] = []

  for (const el of Array.from(rootEl.children)) {
    const made = readCell(el)
    if (!made) continue
    cells.set(made.cell.id!, made.cell)
    links.push(made)
  }

  for (const { cell, parent, source, target } of links) {
    const owner = parent ? cells.get(parent) ?? null : null
    if (owner) owner.insert(cell)
    if (source) cell.setTerminal(cells.get(source) ?? null, true)
    if (target) cell.setTerminal(cells.get(target) ?? null, false)
  }

  // 第一個沒有父親的就是 root（draw.io 慣例是 id="0"）。
  const root = links.find((l) => !l.parent)?.cell
  if (!root) throw new Error('這不是一份 .drawio：沒有 root cell')
  model.setRoot(root)
}

function readCell(el: Element): { cell: Cell; parent: string | null; source: string | null; target: string | null } | null {
  const wrapped = WRAPPERS.has(el.tagName)
  const mx = wrapped ? el.querySelector('mxCell') : el
  if (!mx || mx.tagName !== 'mxCell') return null

  const id = (wrapped ? el.getAttribute('id') : mx.getAttribute('id')) ?? ''
  // 包起來的時候，自訂屬性（`loomId` 就住在這裡）在外層那個元素上，
  // 而 maxGraph 的 cell value 直接收那個元素——跟 draw.io 一樣。
  const value: Element | string = wrapped ? el : (mx.getAttribute('value') ?? '')

  const cell = new Cell(value, readGeometry(mx), parseStyle(mx.getAttribute('style')))
  cell.setId(id)
  if (mx.getAttribute('vertex') === '1') cell.setVertex(true)
  if (mx.getAttribute('edge') === '1') cell.setEdge(true)
  cell.setConnectable(true)

  return {
    cell,
    parent: mx.getAttribute('parent'),
    source: mx.getAttribute('source'),
    target: mx.getAttribute('target'),
  }
}

function readGeometry(mx: Element): Geometry | null {
  const el = mx.querySelector('mxGeometry')
  if (!el) return null

  const num = (name: string) => Number(el.getAttribute(name) ?? 0) || 0
  const geo = new Geometry(num('x'), num('y'), num('width'), num('height'))
  geo.relative = el.getAttribute('relative') === '1'

  const points = Array.from(el.querySelectorAll('Array[as="points"] > mxPoint'))
  if (points.length) {
    geo.points = points.map((p) => new Point(Number(p.getAttribute('x')), Number(p.getAttribute('y'))))
  }
  return geo
}

/**
 * `rounded=1;fillColor=none;` → `{ rounded: 1, fillColor: 'none' }`。
 *
 * 數字保持數字，其餘當字串——跟 maxGraph 自己解析出來的形狀一致，
 * 不然格式面板讀回來的值會跟寫進去的長得不一樣。
 *
 * **渲染時也用這一支**（見 `render.ts`）：畫上去的樣式與存進檔案的樣式走
 * 同一個解析器，才不會出現「畫面看到的」跟「存下去的」不是同一件事。
 */
export function parseStyle(style: string | null): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  for (const part of (style ?? '').split(';')) {
    if (!part) continue
    const at = part.indexOf('=')
    if (at < 0) {
      // `shape;` 這種沒有值的，draw.io 當成形狀名。
      out.shape = part
      continue
    }
    const key = part.slice(0, at)
    const value = part.slice(at + 1)
    out[key] = value !== '' && !Number.isNaN(Number(value)) ? Number(value) : value
  }
  return out
}

/** 產出一份完整的 `.drawio`。 */
export function writeXml(
  model: GraphDataModel,
  { id, name }: { id: string; name: string },
): string {
  const root = model.getRoot()
  const lines: string[] = []

  for (const layer of root?.children ?? []) {
    lines.push(`        <mxCell id="${esc(layer.id ?? '')}" parent="${esc(root!.id ?? '')}"/>`)
    for (const cell of layer.children ?? []) collect(cell, lines)
  }

  return [
    '<mxfile host="diagram-loom">',
    `  <diagram id="${esc(id)}" name="${esc(name)}">`,
    '    <mxGraphModel dx="0" dy="0" grid="1" gridSize="10" page="1" pageWidth="1169" pageHeight="827">',
    '      <root>',
    `        <mxCell id="${esc(root?.id ?? '0')}"/>`,
    ...lines,
    '      </root>',
    '    </mxGraphModel>',
    '  </diagram>',
    '</mxfile>',
  ].join('\n')
}

/** 一個 cell 與它的子孫。父在子之前——draw.io 讀的時候要這個順序。 */
function collect(cell: Cell, out: string[]): void {
  out.push(...one(cell))
  for (const child of cell.children ?? []) collect(child, out)
}

function one(cell: Cell): string[] {
  const style = styleString(cell.getStyle())
  const value = cell.getValue()

  const attrs = [
    style ? `style="${esc(style)}"` : '',
    cell.isVertex() ? 'vertex="1"' : '',
    cell.isEdge() ? 'edge="1"' : '',
    cell.parent ? `parent="${esc(cell.parent.id ?? '')}"` : '',
    cell.source ? `source="${esc(cell.source.id ?? '')}"` : '',
    cell.target ? `target="${esc(cell.target.id ?? '')}"` : '',
  ].filter(Boolean).join(' ')

  const body = cell.getGeometry() ? geometryLines(cell) : []

  // 值是 XML 元素＝帶著自訂屬性（`loomId` 就住在這裡），要包成 `<object>`；
  // 值是字串就直接寫成 `value`。draw.io 自己也是這樣分的。
  if (isElement(value)) {
    const custom = value.getAttributeNames()
      .filter((a) => a !== 'id')
      .map((a) => `${a}="${esc(value.getAttribute(a) ?? '')}"`)
      .join(' ')
    return [
      `        <object ${custom} id="${esc(cell.id ?? '')}">`,
      `          <mxCell ${attrs}>`,
      ...body.map((line) => `  ${line}`),
      '          </mxCell>',
      '        </object>',
    ]
  }

  const label = value == null ? '' : String(value)
  return [
    `        <mxCell id="${esc(cell.id ?? '')}"${label ? ` value="${esc(label)}"` : ''} ${attrs}>`,
    ...body,
    '        </mxCell>',
  ]
}

function isElement(value: unknown): value is Element {
  return Boolean(value) && typeof value === 'object' && 'getAttributeNames' in (value as object)
}

/** 幾何，含轉彎點。**轉彎點掉了，使用者手工繞的線就全部彈回去了。** */
function geometryLines(cell: Cell): string[] {
  const geo = cell.getGeometry()!
  const attrs = [
    geo.relative ? 'relative="1"' : '',
    geo.x ? `x="${round(geo.x)}"` : '',
    geo.y ? `y="${round(geo.y)}"` : '',
    geo.width ? `width="${round(geo.width)}"` : '',
    geo.height ? `height="${round(geo.height)}"` : '',
  ].filter(Boolean).join(' ')

  const points = geo.points ?? []
  if (!points.length) return [`          <mxGeometry ${attrs} as="geometry"/>`]

  return [
    `          <mxGeometry ${attrs} as="geometry">`,
    '            <Array as="points">',
    ...points.map((p) => `              <mxPoint x="${round(p.x)}" y="${round(p.y)}"/>`),
    '            </Array>',
    '          </mxGeometry>',
  ]
}

/** maxGraph 的 style 物件 → draw.io 的 style 字串。 */
function styleString(style: unknown): string {
  if (!style) return ''
  if (typeof style === 'string') return style

  const parts = Object.entries(style as Record<string, unknown>)
    .filter(([, v]) => v != null && v !== '')
    .map(([k, v]) => {
      if (v === true) return `${k}=1`
      if (v === false) return `${k}=0`
      return `${k}=${typeof v === 'number' ? round(v) : String(v)}`
    })
  return parts.length ? `${parts.join(';')};` : ''
}

/**
 * ⚠️ **座標一律四捨五入到小數點後兩位。**
 *
 * `alignCells` 會留下 `12.000000000000028` 這種殘渣（見
 * `docs/editor-widgets.md`），而這個專案的圖會存檔、會 diff、會對帳——
 * 一個「什麼都沒改」的檔案每次存出不同的數字，是最難查的那種雜訊。
 */
function round(n: number): number {
  return Math.round(n * 100) / 100
}

/** XML 的文字跳脫。少了它，名字裡一個 `&` 就會讓整張圖讀不進去。 */
function esc(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/\n/g, '&#10;')
}
