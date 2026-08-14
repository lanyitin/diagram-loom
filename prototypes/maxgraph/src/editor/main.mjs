/**
 * 把 widget 組成一個編輯器。
 *
 * # 這一輪在回答什麼
 *
 * 上一輪的原型只驗「畫得出來、點得到」。這一輪問的是**取代 draw.io 之後，
 * 這個編輯器需要哪些 widget**，目標是讓使用者**快速**產出一張**正確、完整、
 * 容易閱讀**的圖。
 *
 * 版面照那四個目標分：
 *
 * ```text
 * ┌ 工具列 ─ 快 ────────────────────────────────────────────┐
 * ├──────────────────────────────┬─────────────────────────┤
 * │ 形狀庫 │ 畫布                 │ 格式面板 ─ 易讀          │
 * │ ─ 快   │                     │ 模型面板 ─ 正確／完整     │
 * │        │            ┌ 大綱 ┐  │                         │
 * ├──────────────────────────────┴─────────────────────────┤
 * │ 狀態列 ─ 完整（還剩幾個沒指定）                           │
 * └────────────────────────────────────────────────────────┘
 * ```
 *
 * ⚠️ **左邊本來刻意沒有形狀庫**，理由是「圖是從模型產生的，拖形狀建出來的
 * 東西模型不認得」。後來使用者要的是**能自由畫圖**，所以做了——但那個顧慮
 * 沒有消失，只是換了答案：
 *
 * > 拖出來的形狀不帶 `loomId`，所以它會**自動出現在「還沒指定」那份清單裡**。
 *
 * 畫圖是自由的，而「圖上有一個模型不認得的東西」不會被藏起來。
 */

import { Graph, InternalEvent, Outline, UndoManager } from '@maxgraph/core'

import { fixture } from '../fixture.mjs'
import { layout } from '../layout.mjs'
import { render } from '../render.mjs'
import { formatPanel } from './format.mjs'
import { clipboard, connecting, contextMenu, doubleClickToInsert } from './interact.mjs'
import { attribute, editMetadata, setAttribute, useElementValues } from './metadata.mjs'
import { shapePicker } from './shapes.mjs'
import { keyboard, toolbar } from './toolbar.mjs'
import { foldIcons, grid, panAndZoom } from './view.mjs'

const $ = (id) => document.getElementById(id)

// ── 畫布 ────────────────────────────────────────────────────────

const graph = new Graph($('canvas'))
// 這一輪是**編輯器**，所以全部打開——上一個原型是刻意關掉的。
graph.setCellsMovable(true)
graph.setCellsResizable(true)
graph.setCellsEditable(true)

/**
 * 放得進容器。
 *
 * ⚠️ 這條**非開不可**。真實世界的架構圖是巢狀的（中心 → 群組 → 伺服器 →
 * 裡面的小框，三到四層），而 `dropEnabled` 預設是關的——拖一個框到容器上
 * 放開，它只是**疊在上面**，父子關係完全沒變。畫面看起來對，結構是錯的，
 * 而且要等到摺疊容器（或存檔）才會發現。
 */
graph.setDropEnabled(true)
// 子節點被拖到容器外緣時，讓容器跟著長大，而不是把子節點壓回去。
graph.setExtendParents(true)
graph.setExtendParentsOnAdd(true)
graph.setConstrainChildren(false)
// 拖到一條線上不要把那條線切斷——draw.io 有這個，但它很容易誤觸。
graph.setSplitEnabled(false)

// 值可能是 XML 元素（帶自訂屬性）。**這行要在任何東西畫出來之前**，
// 否則標籤會變成 `object`。
useElementValues(graph)

// 摺疊圖示要在畫任何東西之前換掉，否則第一次畫出來的還是壞的那個。
foldIcons(graph)

const paintGrid = grid(graph, $('canvas'))
// 縮放與平移之後狀態列的百分比要跟著動。
panAndZoom(graph, { onView: () => { paintGrid(); status() } })
connecting(graph)

const STYLE = {
  site: { fillColor: 'none', dashed: true, verticalAlign: 'top', fontStyle: 1 },
  machine: { fillColor: 'none', verticalAlign: 'top' },
  instance: { rounded: true, fillColor: '#ffffff' },
  infra: { shape: 'hexagon', perimeter: 'hexagonPerimeter2', fillColor: '#ffffff' },
}
const styleOf = (node) => {
  if (node.id.startsWith('site')) return STYLE.site
  if (node.id === 'f5') return STYLE.infra
  return node.children?.length ? STYLE.machine : STYLE.instance
}

const model = fixture()
const { laid } = await layout(model)
const cells = render(graph, laid, model, styleOf)

// ── 復原 ────────────────────────────────────────────────────────
//
// ⚠️ 這一段是**自己接才知道要寫**的：UndoManager 不會自己聽圖的變更。
// 少了它，工具列上的復原按鈕會安靜地什麼都不做。

const undoManager = new UndoManager()
const remember = (_sender, evt) => undoManager.undoableEditHappened(evt.getProperty('edit'))
graph.getDataModel().addListener(InternalEvent.UNDO, remember)
graph.getView().addListener(InternalEvent.UNDO, remember)

// ── 新增形狀 ────────────────────────────────────────────────────
//
// ⚠️ **拖出來的形狀不帶 `loomId`**，所以它會自動落在「還沒指定」那份清單裡。
// 這是「圖上新增一個框」對模型的意思：不是新增一台機器，
// 是**多了一件還沒交代的事**。

let made = 0
function insert(shape, x, y) {
  const view = graph.getView()
  const box = graph.container.getBoundingClientRect()
  // 沒給位置（點一下而不是拖曳）就放在畫面中央。
  const at = x != null
    ? { x, y }
    : graph.getPointForEvent({ clientX: box.left + box.width / 2, clientY: box.top + box.height / 2 })
  void view

  let cell
  graph.batchUpdate(() => {
    cell = graph.insertVertex({
      parent: graph.getDefaultParent(),
      id: `new-${made += 1}`,
      value: shape.label,
      position: [Math.round(at.x - shape.w / 2), Math.round(at.y - shape.h / 2)],
      size: [shape.w, shape.h],
      style: { ...shape.style },
    })
  })
  graph.setSelectionCell(cell)
  status()
  return cell
}

const picker = await shapePicker(graph, $('shapes'), { insert })

// ── 格式面板與工具列 ────────────────────────────────────────────

const refresh = () => { format.refresh(); status() }
const format = formatPanel(graph, { onChange: () => status() })
$('format').append(format.el)

const clip = clipboard(graph, { onChange: refresh })
const openData = (cell) => editMetadata(graph, cell, { onChange: refresh })

$('bar').append(toolbar(graph, undoManager, {
  onChange: refresh,
  clip,
  onEditData: () => {
    const [cell] = graph.getSelectionCells()
    if (cell) openData(cell)
  },
}))
keyboard(graph, undoManager, { onChange: refresh, clip, onEditData: openData })
contextMenu(graph, { clip, onEditData: openData, onChange: refresh })
doubleClickToInsert(graph, insert)

graph.getSelectionModel().addListener(InternalEvent.CHANGE, () => {
  format.refresh()
  status()
})
// 圖被別的路徑改掉（復原、對齊、程式產生）時面板也要跟著——
// 面板停在舊值的話，使用者會相信它寫的那個。
graph.getDataModel().addListener(InternalEvent.CHANGE, () => format.refresh())

// ── 大綱 ────────────────────────────────────────────────────────
//
// 幾百個節點的圖一定要縮到看不見字才看得完整張。大綱讓使用者**知道自己
// 在哪**——這是「容易閱讀」裡最便宜的一個。maxGraph 內建。

const outline = new Outline(graph, $('outline'))

// ── 模型面板：正確與完整 ────────────────────────────────────────
//
// **這一塊是 draw.io 沒有、也不可能有的。** 格式面板讓圖好看，
// 這塊讓圖說實話：哪些形狀還沒接上模型。

const TARGETS = [
  { id: 'i-order-api', label: 'order-api' },
  { id: 'i-redis-01', label: 'redis-01' },
  { id: 'i-redis-02', label: 'redis-02' },
  { id: 'i-redis-03', label: 'redis-03' },
  { id: 'n-f5', label: 'f5-01' },
]

/**
 * 綁定**只存在形狀的 `loomId` 屬性上**。
 *
 * ⚠️ 第一版把它記在畫面的一個 `Map` 裡，於是「編輯資料」打開來看不到
 * `loomId`——因為圖上真的沒有。畫面說已經指定好了，圖上什麼都沒有，
 * 對帳當然也看不見。**那是這個工具最不能出的那種錯。**
 *
 * 現在畫面不記任何東西，每次都回頭問圖。
 */
function boundTo(cell) {
  return attribute(cell, 'loomId')
}

/** 圖上已經被指定走的元素。跟正式版的 `boundShapes(xml)` 是同一件事。 */
function taken() {
  return new Set(
    graph.getDefaultParent()
      .filterDescendants((c) => c.isVertex())
      .map(boundTo)
      .filter(Boolean),
  )
}

function modelPanel() {
  const list = $('targets')
  list.textContent = ''
  const selected = graph.getSelectionCells().filter((c) => c.isVertex())
  const used = taken()
  const current = selected.length === 1 ? boundTo(selected[0]) : null

  for (const target of TARGETS) {
    const isTaken = used.has(target.id)
    const isThisOne = current === target.id
    const li = document.createElement('li')
    const name = document.createElement('span')
    name.textContent = target.label
    name.className = isTaken ? 'picked' : ''

    const button = document.createElement('button')
    button.textContent = isThisOne ? '取消' : (isTaken ? '已指定' : '就是它')
    // 指定過的還是要能收回來——指錯了不能變成永久事實。
    button.disabled = selected.length !== 1 || (isTaken && !isThisOne)
    button.dataset.target = target.id
    button.onclick = () => {
      const cell = selected[0]
      setAttribute(graph, cell, 'loomId', isThisOne ? null : target.id)
      graph.batchUpdate(() => {
        graph.setCellStyles('strokeColor', isThisOne ? null : '#b4552d', [cell])
        graph.setCellStyles('strokeWidth', isThisOne ? null : 2, [cell])
      })
      modelPanel()
      status()
    }
    li.append(name, button)
    list.append(li)
  }
}

// ── 狀態列：把「還剩幾個」一直放在眼前 ──────────────────────────
//
// 這個工具的命是怕漏。「還剩幾個沒指定」不能只在面板裡講——
// 使用者不會去打開一個他不知道自己需要的面板。

/**
 * 圖上現在有幾個形狀。
 *
 * ⚠️ **不能用一開始畫進去的那個數字**：使用者自己拖出來的形狀也要算，
 * 不然「還剩幾個沒指定」會在他最需要它的時候說謊——他剛剛才多畫一個。
 */
function shapeCount() {
  return graph.getDefaultParent().filterDescendants((c) => c.isVertex()).length
}

function status() {
  const selected = graph.getSelectionCells()
  const total = shapeCount()
  const left = total - taken().size
  // 縮放百分比放在最前面：使用者回報過「沒有 zoom in/out」，而當時它其實有，
  // 只是**畫面上沒有任何地方看得出現在幾 %**，也就沒辦法知道它有沒有反應。
  $('status').textContent = [
    `${Math.round(graph.getView().scale * 100)}%`,
    `${total} 個形狀`,
    left ? `還有 ${left} 個沒指定` : '全部指定完了',
    selected.length ? `選了 ${selected.length} 個` : '',
  ].filter(Boolean).join('　·　')
  modelPanel()
}

format.refresh()
status()
paintGrid()
graph.getPlugin('fit').fitCenter({ maxScale: 1.2 })

/** 探針要問「你到底組出了什麼」。 */
window.__editor = {
  graph,
  undoManager,
  clip,
  openData,
  insert,
  outline: Boolean(outline),
  cells: cells.size,
  shapeCount,
  boundTo,
  shapes: picker.count,
  widgets: ['toolbar', 'shapes', 'format', 'model', 'outline', 'status', 'keyboard', 'grid'],
}
