/**
 * 原型的互動半邊：**問題二與問題三只有人用得出答案**，機器判斷不了。
 *
 * - 問題二：少了 draw.io 的編輯器，到底有多痛？
 * - 問題三：點形狀 → 標註它代表誰，做起來多快？
 *
 * 所以這支程式本身就是答案的一部分——**它有多長，代價就是多少**。
 * 每一段前面都標了「這段在回答什麼」。
 *
 * 順帶把問題四從「解析得了」推到「畫得出來」：探針證明 stencils 的 XML
 * 讀得進去，但讀得進去不等於畫得出來。這裡真的拿一個 draw.io 的形狀來畫。
 */

import { Graph, InternalEvent, StencilShape, StencilShapeRegistry } from '@maxgraph/core'

import { fixture } from './fixture.mjs'
import { layout, walk } from './layout.mjs'
// 這個檔案自己有一個 render()（重畫側邊欄），所以改個名字進來。
import { render as draw } from './render.mjs'

const log = (line) => {
  document.getElementById('log').textContent += `${line}\n`
}

// ── 問題三的材料：模型裡有哪些元素可以指 ────────────────────────
//
// 主程式那邊這份清單是 Rust 算的（`annotate.rs`）。原型只要一份假的，
// 因為要驗的是「點得到形狀嗎」，不是「清單對不對」。

const TARGETS = [
  { id: 'i-order-api', label: 'order-api' },
  { id: 'i-redis-01', label: 'redis-01' },
  { id: 'i-redis-02', label: 'redis-02' },
  { id: 'i-redis-03', label: 'redis-03' },
  { id: 'n-f5', label: 'f5-01' },
]

/** 哪個 cell 綁到哪個元素。主程式那邊這件事寫在 XML 的 `loomId` 上。 */
const bound = new Map()
let selected = null

// ── 問題一的另一半：排完的座標真的畫得出來嗎 ────────────────────
//
// 探針量的是座標的正確性，這裡驗的是「同一份座標交給 maxGraph 畫得對」。

const container = document.getElementById('canvas')
const graph = new Graph(container)
graph.setPanning(true)
// 一開始就把「改不得」關掉。這是原型，不是編輯器——要驗的是選取。
graph.setCellsMovable(false)
graph.setCellsResizable(false)
graph.setCellsEditable(false)

const STYLE = {
  site: { fillColor: 'none', dashed: true, verticalAlign: 'top', fontStyle: 1 /* 粗體。FONT_STYLE_MASK 有定義，但 index.js 沒有 re-export */ },
  machine: { fillColor: 'none', verticalAlign: 'top' },
  instance: { rounded: true, fillColor: '#ffffff' },
  infra: { shape: 'hexagon', perimeter: 'hexagonPerimeter2', fillColor: '#ffffff' },
}

function styleOf(node) {
  if (node.id.startsWith('site')) return STYLE.site
  if (node.id === 'f5') return STYLE.infra
  return node.children?.length ? STYLE.machine : STYLE.instance
}

const model = fixture()
const { laid, ms } = await layout(model)
log(`elkjs 排版 ${Math.round(ms)} ms，整張圖 ${Math.round(laid.width)}×${Math.round(laid.height)}`)

const cells = draw(graph, laid, model, styleOf)

let drawn = 0
walk(laid, () => { drawn += 1 })
log(`畫出 ${drawn} 個節點、${model.edges.length} 條線`)

// ── 問題三：點形狀 → 標註它代表誰 ───────────────────────────────
//
// **這就是整件事的動機。** draw.io 的嵌入協定沒有選取事件，站在 iframe
// 外面永遠不知道使用者點了哪個形狀；這裡是一個 addListener。

graph.addListener(InternalEvent.CLICK, (_sender, evt) => {
  const cell = evt.getProperty('cell')
  selected = cell && cell.isVertex() ? cell : null
  render()
})

function assign(target) {
  if (!selected) return
  bound.set(selected.id, target)
  // 綁定要看得出來。主程式那邊是改 XML 字串，這裡是改一個屬性。
  graph.batchUpdate(() => {
    graph.setCellStyles('strokeColor', '#b4552d', [selected])
    graph.setCellStyles('strokeWidth', '2', [selected])
    graph.model.setValue(selected, `${selected.id}\n= ${target.label}`)
  })
  render()
}

function render() {
  const left = cells.size - bound.size
  document.getElementById('left').textContent = `還剩 ${left} 個沒指定`
  document.getElementById('hint').textContent = selected
    ? `選中：${selected.id}`
    : '在左邊點一個方框。'

  const list = document.getElementById('targets')
  list.textContent = ''
  for (const target of TARGETS) {
    const taken = [...bound.values()].includes(target)
    const li = document.createElement('li')
    const name = document.createElement('span')
    name.textContent = target.label
    name.style.flex = '1'
    if (taken) name.className = 'picked'
    const button = document.createElement('button')
    button.textContent = taken ? '已指定' : '就是它'
    button.disabled = !selected || taken
    button.dataset.target = target.id
    button.onclick = () => assign(target)
    li.append(name, button)
    list.append(li)
  }
}
render()

// ── 問題四：讀得進去，也畫得出來嗎 ──────────────────────────────
//
// vendor/drawio 不在版控裡，所以拿不到檔案是正常的——那時候要說話，
// 而不是安靜地少畫一個形狀。

/** 探針要問「你到底做到了什麼」。畫面上看得到的東西不夠精確。 */
window.__probe = { nodes: drawn, edges: model.edges.length, layoutMs: ms, stencils: 0 }

try {
  const res = await fetch('/stencils/arrows.xml')
  if (!res.ok) throw new Error(`HTTP ${res.status}`)
  const doc = new DOMParser().parseFromString(await res.text(), 'text/xml')
  // ⚠️ 拿回一份 index.html 的話，`res.ok` 一樣是 true，解析出 0 個形狀，
  // 然後一切「正常」地少畫一個東西。所以這裡要認一下拿到的是不是 stencil。
  if (doc.documentElement.tagName !== 'shapes') {
    throw new Error(`拿回來的不是 stencil（根元素是 ${doc.documentElement.tagName}）`)
  }

  let count = 0
  for (const el of doc.documentElement.children) {
    if (el.tagName !== 'shape') continue
    // 名字照 draw.io 的規矩組：<shapes name> + <shape name>，小寫、空白換底線。
    const name = `${doc.documentElement.getAttribute('name')}.${el.getAttribute('name')}`
      .toLowerCase()
      .replace(/ /g, '_')
    StencilShapeRegistry.add(name, new StencilShape(el))
    count += 1
  }
  log(`載入 ${count} 個 draw.io 形狀（arrows.xml）`)
  window.__probe.stencils = count
  // 註冊完查得回來才算數。解析成功不等於畫得出來——CellRenderer 是拿
  // style.shape 去 StencilShapeRegistry 查的，查不到就默默退回矩形。
  window.__probe.resolved = Boolean(StencilShapeRegistry.get('mxgraph.arrows.arrow_down'))

  graph.batchUpdate(() => {
    graph.insertVertex({
      parent: graph.getDefaultParent(),
      id: 'stencil-demo',
      value: 'draw.io 的形狀',
      position: [laid.width + 60, 40],
      size: [70, 98],
      style: { shape: 'mxgraph.arrows.arrow_down', verticalLabelPosition: 'bottom', verticalAlign: 'top' },
    })
  })
  log('畫了一個 mxgraph.arrows.arrow_down')
} catch (e) {
  log(`形狀庫載入失敗：${e.message}`)
}

// ⚠️ `graph.fit()` 在 0.24 已經不是 Graph 的方法了，搬進 FitPlugin。
// 它是**執行期**才炸的（fit is not a function），而且炸在最後一行——
// 前面全部畫完了，看起來只是「沒有自動縮放」。
graph.getPlugin('fit').fitCenter({ maxScale: 1.2 })
