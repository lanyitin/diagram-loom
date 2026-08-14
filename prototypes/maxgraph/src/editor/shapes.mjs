/**
 * 形狀庫：draw.io 左邊那一欄。
 *
 * # 為什麼原本說不做，現在做了
 *
 * `docs/editor-widgets.md` 一開始的判斷是「圖是從模型產生的，不該讓使用者
 * 拖形狀」。**使用者要的是能自由畫圖**，所以做，但那個顧慮沒有消失，
 * 只是換了答案：
 *
 * > **拖出來的形狀不帶 `loomId`，所以它會自動出現在「還沒指定」那份清單裡。**
 *
 * 這樣兩邊都成立：畫圖是自由的，而「圖上有一個模型不認得的東西」這件事
 * 不會被藏起來——它會被算進「還剩幾個沒指定」。
 *
 * # 預覽是真的畫出來的
 *
 * 每一格都是一個很小的 `Graph`，把那個形狀真的畫一次。用圖片或手寫 SVG
 * 比較省，但那樣**預覽會跟實際畫出來的不一樣**——而形狀庫的唯一功能就是
 * 「讓你先看到它長什麼樣」。
 *
 * ⚠️ 代價是每一格一個 Graph 實例。正式版該把預覽預先算成 SVG 精靈圖，
 * 這裡是原型，先求正確。
 */

import { Graph, StencilShape, StencilShapeRegistry, gestureUtils } from '@maxgraph/core'

/** 我們自己的模型形狀。放第一組，因為那是這個工具最常畫的東西。 */
const MODEL_SHAPES = [
  { label: '站點', w: 240, h: 160, style: { fillColor: 'none', dashed: true, verticalAlign: 'top', fontStyle: 1 } },
  { label: '機器', w: 200, h: 120, style: { fillColor: 'none', verticalAlign: 'top' } },
  { label: '服務', w: 160, h: 60, style: { rounded: true, fillColor: '#ffffff' } },
  { label: '設備', w: 160, h: 60, style: { shape: 'hexagon', perimeter: 'hexagonPerimeter2', fillColor: '#ffffff' } },
  { label: '外部系統', w: 160, h: 60, style: { rounded: true, dashed: true, fillColor: '#ffffff' } },
  { label: '人', w: 40, h: 70, style: { shape: 'actor', verticalLabelPosition: 'bottom', verticalAlign: 'top' } },
]

/** 通用形狀。draw.io 的 basic 那一組，大家都在用的那幾個。 */
const BASIC_SHAPES = [
  { label: '方框', w: 120, h: 60, style: {} },
  { label: '圓角', w: 120, h: 60, style: { rounded: true } },
  { label: '橢圓', w: 120, h: 60, style: { shape: 'ellipse' } },
  { label: '菱形', w: 100, h: 100, style: { shape: 'rhombus' } },
  { label: '三角形', w: 80, h: 80, style: { shape: 'triangle' } },
  { label: '圓柱', w: 80, h: 100, style: { shape: 'cylinder' } },
  { label: '雲', w: 120, h: 80, style: { shape: 'cloud' } },
  { label: '便利貼', w: 100, h: 80, style: { shape: 'note', fillColor: '#fff2cc' } },
]

/** 從 draw.io 的 stencil 檔挑幾個常用的。名字照 draw.io 的規矩組。 */
const STENCIL_PICKS = [
  ['mxgraph.networks.server', '伺服器', 80, 100],
  ['mxgraph.networks.firewall', '防火牆', 100, 60],
  ['mxgraph.networks.switch', '交換器', 100, 60],
  ['mxgraph.networks.router', '路由器', 80, 80],
  ['mxgraph.networks.load_balancer', '負載平衡', 80, 100],
  ['mxgraph.networks.cloud', '網際網路', 120, 80],
]

/**
 * 建形狀庫。
 *
 * `insert(style, w, h, x, y)` 由外面提供——放哪裡、要不要進復原、
 * 要不要選起來，那些是編輯器的事，不是形狀庫的事。
 */
export async function shapePicker(graph, container, { insert }) {
  const stencils = await loadStencils()

  const sections = [
    ['模型', MODEL_SHAPES],
    ['基本', BASIC_SHAPES],
    ['網路（draw.io 形狀庫）', stencils],
  ]

  for (const [title, shapes] of sections) {
    if (!shapes.length) continue
    const head = document.createElement('h2')
    head.textContent = title
    const grid = document.createElement('div')
    grid.className = 'palette'

    for (const shape of shapes) {
      grid.append(tile(graph, shape, insert))
    }
    container.append(head, grid)
  }

  return { count: sections.reduce((n, [, s]) => n + s.length, 0) }
}

/** 一格：預覽 + 名字。點一下插到畫面中央，也可以直接拖到畫布上。 */
function tile(graph, shape, insert) {
  const el = document.createElement('button')
  el.className = 'tile'
  el.title = shape.label
  el.dataset.shape = shape.label

  const preview = document.createElement('div')
  preview.className = 'preview'
  el.append(preview)

  const name = document.createElement('span')
  name.textContent = shape.label
  el.append(name)

  // 點一下＝插到畫面中央。draw.io 也是這樣，而且比拖曳快。
  el.onclick = () => insert(shape)

  // 拖曳＝插到放開的位置。
  gestureUtils.makeDraggable(el, graph, (_graph, _evt, _target, x, y) => insert(shape, x, y))

  // 預覽晚一步畫，先把版面長出來——16 個小 Graph 一起建會卡一下。
  queueMicrotask(() => drawPreview(preview, shape))
  return el
}

/** 用一個很小的 Graph 把形狀真的畫一次。 */
function drawPreview(container, shape) {
  const graph = new Graph(container)
  graph.setEnabled(false)
  graph.setPanning(false)
  const scale = Math.min(46 / shape.w, 34 / shape.h)
  graph.batchUpdate(() => {
    graph.insertVertex({
      parent: graph.getDefaultParent(),
      value: '',
      position: [2, 2],
      size: [shape.w * scale, shape.h * scale],
      style: shape.style,
    })
  })
}

/**
 * 把 draw.io 的 stencil 檔讀進來。
 *
 * 拿不到檔案是正常的（`vendor/drawio` 不進版控），那時候這一組就不出現，
 * **而不是出現一排空白方框**——查不到 stencil 時 maxGraph 會默默退回矩形。
 */
async function loadStencils() {
  try {
    const res = await fetch('/stencils/networks.xml')
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    const doc = new DOMParser().parseFromString(await res.text(), 'text/xml')
    if (doc.documentElement.tagName !== 'shapes') throw new Error('拿回來的不是 stencil')

    const prefix = doc.documentElement.getAttribute('name')
    for (const el of doc.documentElement.children) {
      if (el.tagName !== 'shape') continue
      const name = `${prefix}.${el.getAttribute('name')}`.toLowerCase().replace(/ /g, '_')
      StencilShapeRegistry.add(name, new StencilShape(el))
    }

    return STENCIL_PICKS
      .filter(([name]) => StencilShapeRegistry.get(name))
      .map(([name, label, w, h]) => ({ label, w, h, style: { shape: name } }))
  } catch {
    return []
  }
}
