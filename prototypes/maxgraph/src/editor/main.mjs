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
 * │ 畫布                          │ 格式面板 ─ 易讀          │
 * │                              │ 模型面板 ─ 正確／完整     │
 * │                     ┌ 大綱 ┐  │                         │
 * ├──────────────────────────────┴─────────────────────────┤
 * │ 狀態列 ─ 完整（還剩幾個沒指定）                           │
 * └────────────────────────────────────────────────────────┘
 * ```
 *
 * **左邊沒有形狀庫。** draw.io 那一欄在這裡沒有意義：圖是從模型產生的，
 * 使用者不該用拖形狀的方式新增一台機器——那樣建出來的東西模型不認得。
 * 要新增東西是去「資源」那一頁，或用工具列的樣板（尚未做）。
 */

import { Graph, InternalEvent, Outline, UndoManager } from '@maxgraph/core'

import { fixture } from '../fixture.mjs'
import { layout } from '../layout.mjs'
import { render } from '../render.mjs'
import { formatPanel } from './format.mjs'
import { keyboard, toolbar } from './toolbar.mjs'

const $ = (id) => document.getElementById(id)

// ── 畫布 ────────────────────────────────────────────────────────

const graph = new Graph($('canvas'))
graph.setPanning(true)
// 這一輪是**編輯器**，所以全部打開——上一個原型是刻意關掉的。
graph.setCellsMovable(true)
graph.setCellsResizable(true)
graph.setCellsEditable(true)
graph.setConnectable(false)
graph.setDropEnabled(false)

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
const { laid, ms } = await layout(model)
const cells = render(graph, laid, model, styleOf)

// ── 復原 ────────────────────────────────────────────────────────
//
// ⚠️ 這一段是**自己接才知道要寫**的：UndoManager 不會自己聽圖的變更。
// 少了它，工具列上的復原按鈕會安靜地什麼都不做。

const undoManager = new UndoManager()
const remember = (_sender, evt) => undoManager.undoableEditHappened(evt.getProperty('edit'))
graph.getDataModel().addListener(InternalEvent.UNDO, remember)
graph.getView().addListener(InternalEvent.UNDO, remember)

// ── 格式面板與工具列 ────────────────────────────────────────────

const format = formatPanel(graph, { onChange: () => status() })
$('format').append(format.el)
$('bar').append(toolbar(graph, undoManager, { onChange: () => { format.refresh(); status() } }))
keyboard(graph, undoManager, { onChange: () => { format.refresh(); status() } })

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
const bound = new Map()

function modelPanel() {
  const list = $('targets')
  list.textContent = ''
  const selected = graph.getSelectionCells().filter((c) => c.isVertex())

  for (const target of TARGETS) {
    const taken = [...bound.values()].includes(target.id)
    const li = document.createElement('li')
    const name = document.createElement('span')
    name.textContent = target.label
    name.className = taken ? 'picked' : ''
    const button = document.createElement('button')
    button.textContent = taken ? '已指定' : '就是它'
    button.disabled = selected.length !== 1 || taken
    button.dataset.target = target.id
    button.onclick = () => {
      bound.set(selected[0].id, target.id)
      graph.batchUpdate(() => {
        graph.setCellStyles('strokeColor', '#b4552d', selected)
        graph.setCellStyles('strokeWidth', 2, selected)
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

function status() {
  const selected = graph.getSelectionCells()
  const left = cells.size - bound.size
  $('status').textContent = [
    `${cells.size} 個形狀`,
    left ? `還有 ${left} 個沒指定` : '全部指定完了',
    selected.length ? `選了 ${selected.length} 個` : '',
    `排版 ${Math.round(ms)} ms`,
  ].filter(Boolean).join('　·　')
  modelPanel()
}

format.refresh()
status()
graph.getPlugin('fit').fitCenter({ maxScale: 1.2 })

/** 探針要問「你到底組出了什麼」。 */
window.__editor = {
  graph,
  undoManager,
  outline: Boolean(outline),
  cells: cells.size,
  widgets: ['toolbar', 'format', 'model', 'outline', 'status', 'keyboard'],
}
