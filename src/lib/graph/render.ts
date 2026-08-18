/**
 * 模型 → maxGraph 的資料模型。
 *
 * 這支取代了以前的 `toXml()` 那條路：以前是「模型 → XML 字串 → 丟給 iframe」，
 * 現在是「模型 → cells」，而 XML 只在**存檔**時才出現（見 `mxml.ts`）。
 *
 * # 接在資料模型上，不接在畫面上
 *
 * 參數是 [`GraphDataModel`] 而不是 `Graph`——建 cell 不需要畫面，
 * 而**不需要畫面的東西就能進快速測試層**。畫面那一層只要 `new Graph(容器, 模型)`。
 *
 * # 每個形狀都要帶 `loomId`
 *
 * 對帳靠的就是它，而規矩跟 draw.io 那時候一模一樣（見 `diagram.ts` 的模組註解）：
 *
 * - **沒有 `loomId` 的形狀，對帳看不見**——那正好拿來畫純裝飾的東西
 * - **一個模型元素 ↔ 一個形狀**，但**線是例外**：一條萬用字元連線是 N×M 條線，
 *   全部共用同一個 `loomId`
 *
 * 自訂屬性放在 cell 的 value（一個 XML 元素）上，跟 draw.io 的
 * `<object loomId=...>` 是同一件事——所以存檔時原封不動寫得回去。
 */

import { Cell, Geometry, type GraphDataModel, Point } from '@maxgraph/core'

import type { Shape } from '../diagram'
import type { Link } from '../model'
import { EDGE } from '../diagram'
import { edgeId, type Laid, type Placed } from './layout'
import { parseStyle } from './mxml'

/**
 * 把排完版的模型畫進資料模型。回傳 id → cell 的對照表。
 *
 * 呼叫前資料模型會被清空——重畫就是重畫，不做增量更新：增量更新要回答
 * 「這個形狀是同一個嗎」，而那個問題正是對帳在回答的，不該有第二套答案。
 */
export function render(
  model: GraphDataModel,
  shapes: Shape[],
  links: Link[],
  laid: Laid,
): Map<string, Cell> {
  const byId = new Map(shapes.map((s) => [s.id, s]))
  const cells = new Map<string, Cell>()

  const root = new Cell()
  root.setId('0')
  const layer = new Cell()
  layer.setId('1')
  root.insert(layer)

  const place = (node: Placed, parent: Cell) => {
    const shape = byId.get(node.id)
    if (!shape) return

    const cell = new Cell(
      valueOf(shape),
      new Geometry(node.x, node.y, node.width, node.height),
      parseStyle(shape.style),
    )
    cell.setId(shape.id)
    cell.setVertex(true)
    cell.setConnectable(true)
    parent.insert(cell)
    cells.set(shape.id, cell)

    for (const child of node.children) place(child, cell)
  }
  for (const node of laid.nodes) place(node, layer)

  const routes = new Map(laid.edges.map((e) => [e.id, e]))
  links.forEach((link, i) => {
    const source = cells.get(link.from)
    const target = cells.get(link.to)
    // 兩端都要畫得出來。指向不存在的形狀的線會變成一條飄在畫布上的浮線，
    // 看起來像連到別的地方——比不畫更糟。
    if (!source || !target) return

    const geometry = new Geometry()
    geometry.relative = true

    // ⚠️ ELK 算好的轉彎點一定要寫回來。不寫的話 maxGraph 會自己再繞一次，
    // 而它不知道容器在哪——實測線會直接穿過三層容器。
    // 鍵由 `edgeId` 產生，`i` 是**原始陣列**的位置——排版那邊也是。
    // 兩邊各自編號的話，只要有一條線被丟掉就會整批錯開（見 `edgeId`）。
    const bends = routes.get(edgeId(link, i))?.bends ?? []
    if (bends.length) geometry.points = bends.map((p) => new Point(p.x, p.y))

    const cell = new Cell(
      edgeValue(link),
      geometry,
      // 有轉彎點時**不要再給 edgeStyle**：ELK 給的已經是完整的正交折線，
      // 再套一個繞線器等於兩個東西同時決定線怎麼走，而贏的是不知道容器
      // 在哪的那個。
      parseStyle(bends.length ? withoutRouter(link) : styleOf(link)),
    )
    // 一條萬用字元連線長出多條線，所以 cell 的 id 要帶上是哪一對，
    // 而 `loomId` 仍然是那一條連線——對帳認的是後者。
    cell.setId(
      link.connection ? `${link.connection}:${link.from}:${link.to}` : `${link.from}:${link.to}`,
    )
    cell.setEdge(true)
    cell.setTerminal(source, true)
    cell.setTerminal(target, false)
    layer.insert(cell)
    cells.set(cell.id!, cell)
  })

  model.setRoot(root)
  return cells
}

/** 形狀的值：一個帶自訂屬性的元素，跟 draw.io 的 `<object>` 一樣。 */
function valueOf(shape: Shape): Element {
  // 副標（位址、種類）畫在名字下面。這張圖最常被拿去核對的就是位址。
  const label = shape.detail ? `${shape.label}\n${shape.detail}` : shape.label
  return element({ label, loomId: shape.loomId, loomKind: shape.kind })
}

/**
 * 線的值。
 *
 * `connection` 是空的表示這條線**不代表單一模型元素**——context 圖把 A 對 B
 * 的三條契約收成一條線就是這種。這時候刻意不給 `loomId`：硬指其中一條
 * 等於說謊，而對帳會把綁定當事實。
 */
function edgeValue(link: Link): Element {
  return element({
    label: link.purpose,
    loomId: link.connection || undefined,
    loomKind: 'connection',
  })
}

function element(attrs: Record<string, string | undefined>): Element {
  const doc = new DOMParser().parseFromString('<object/>', 'text/xml')
  const el = doc.documentElement
  for (const [name, value] of Object.entries(attrs)) {
    // `loomKind` 標「這個形狀是我們畫的」，`loomId` 標「它是哪個模型元素」。
    // 兩者分開才有辦法畫出「人」——它是我們畫的，但不是環境層的元素，
    // 給它 `loomId` 的話對帳會說「圖上有這個、模型沒有」，一個假的缺漏。
    if (value != null) el.setAttribute(name, value)
  }
  return el
}

function styleOf(link: Link): string {
  return link.kind === 'fallback' ? EDGE.fallback : EDGE.primary
}

/** 拿掉 `edgeStyle=`，其餘照舊——備援線的虛線與灰色要留著。 */
function withoutRouter(link: Link): string {
  return styleOf(link)
    .split(';')
    .filter((part) => part !== '' && !part.startsWith('edgeStyle='))
    .join(';')
}
