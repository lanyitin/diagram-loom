/**
 * 畫布的設定：手感、連線、摺疊、復原。
 *
 * 這些是「`new Graph(容器)` 之後還得自己補的東西」。原型一項一項踩出來的，
 * 每一條都有一個**功能在但使用者碰不到**的故事（見 `docs/editor-widgets.md`）。
 * 搬過來時保留了那些 ⚠️——它們是這個檔案存在的理由。
 */

import {
  type AbstractGraph,
  type ConnectionHandler,
  EdgeHandlerConfig,
  type Graph,
  ImageBox,
  InternalEvent,
  type PanningHandler,
  Point,
  UndoManager,
} from '@maxgraph/core'

/** 格線間距。draw.io 預設也是 10。 */
const GRID = 10

/**
 * 讓圖看得懂「值是一個 XML 元素」。
 *
 * ⚠️ **這一行要在畫任何東西之前。** 我們的形狀值是 `<object label loomId>`，
 * 而 `convertValueToString` 的預設實作遇到節點會回 `nodeName`——於是圖上
 * 每個形狀的字都變成 `object`。不會報錯，只是圖爛掉。
 */
export function useElementValues(graph: AbstractGraph): void {
  graph.convertValueToString = (cell) => {
    const value = cell.getValue() as unknown
    if (isElement(value)) return value.getAttribute('label') ?? ''
    return value == null ? '' : String(value)
  }

  // 就地編輯（雙擊改字）也要跟著：改的是 `label` 屬性，不是整個值。
  const fallback = graph.labelChanged.bind(graph)
  graph.labelChanged = (cell, label, evt) => {
    const value = cell.getValue() as unknown
    if (isElement(value)) {
      const next = value.cloneNode(true) as Element
      next.setAttribute('label', String(label))
      graph.model.setValue(cell, next)
      return cell
    }
    return fallback(cell, label, evt)
  }
}

function isElement(value: unknown): value is Element {
  return Boolean(value) && typeof value === 'object' && 'getAttribute' in (value as object)
}

/**
 * 平移與縮放。
 *
 * | 動作 | 做什麼 |
 * | --- | --- |
 * | 空白處拖曳 | 平移 |
 * | 滾輪 | **以游標為中心縮放** |
 * | ⇧ + 滾輪 | 左右平移 |
 *
 * ⚠️ **滾輪的預設是縮放，不是捲動。** 原型第一版照 draw.io 網頁版做成
 * 「滾輪捲動」，使用者的回報是「沒有 zoom in/out」——他滾了滾輪，畫面只是
 * 平移。這裡的畫布是無限大的：捲動沒有盡頭也沒有參考點，而平移已經有
 * 「拖空白處」這個更直接的手勢。
 */
export function panAndZoom(graph: Graph, onView: () => void): void {
  graph.setPanning(true)

  const panning = graph.getPlugin<PanningHandler>('PanningHandler')!
  // 空白處按著左鍵就能平移；按在形狀上仍然是搬形狀。
  panning.useLeftButtonForPanning = true
  panning.ignoreCell = false
  // 右鍵留給選單，不要拿去平移。
  panning.usePopupTrigger = false

  // 對齊格線。少了它，手動拖出來的東西永遠差幾個像素，
  // 而「容易閱讀」有一半就是對齊。
  graph.setGridEnabled(true)
  graph.gridSize = GRID

  InternalEvent.addMouseWheelListener((evt, up) => {
    const event = evt as WheelEvent
    if (!graph.container.contains(event.target as Node)) return

    if (event.shiftKey) {
      const view = graph.getView()
      const t = view.getTranslate()
      // 除以 scale：放大之後同樣的滾輪格數應該走一樣的**螢幕**距離。
      const step = 48 / view.scale
      view.setTranslate(t.x + (up ? step : -step), t.y)
    } else {
      zoomAt(graph, event, up)
    }
    // 不擋掉的話整個頁面會跟著捲，畫布看起來像「卡住又亂跳」。
    event.preventDefault()
    onView()
  }, graph.container)

  const view = graph.getView()
  view.addListener(InternalEvent.SCALE, onView)
  view.addListener(InternalEvent.TRANSLATE, onView)
  view.addListener(InternalEvent.SCALE_AND_TRANSLATE, onView)
}

/**
 * 以游標為中心縮放。
 *
 * `graph.zoomIn()` 是**以畫布中心**縮放的，用滾輪放大時你盯著的那個東西
 * 會滑走。
 */
function zoomAt(graph: Graph, evt: WheelEvent, up: boolean): void {
  const view = graph.getView()
  const before = view.scale
  const after = up ? before * graph.zoomFactor : before / graph.zoomFactor

  const box = graph.container.getBoundingClientRect()
  const px = evt.clientX - box.left
  const py = evt.clientY - box.top

  const t = view.getTranslate()
  // 螢幕座標 =（圖座標 + 平移）× 縮放
  const gx = px / before - t.x
  const gy = py / before - t.y
  view.scaleAndTranslate(after, px / after - gx, py / after - gy)
}

/**
 * 背景格線。
 *
 * maxGraph **沒有內建可見的格線**（`setGridEnabled` 只管吸附）。
 *
 * ⚠️ **格線一定要是畫布自己的 `background`，不能是一層蓋上去的 div。**
 * 蓋上去的那個是 `absolute`、maxGraph 的 `<svg>` 是 `static`，而**定位過的
 * 元素會畫在未定位的內容之上，跟 DOM 順序無關**——結果是格線壓在圖上面，
 * 穿過每一個形狀跟每一行字。
 */
export function paintGrid(graph: Graph, element: HTMLElement): () => void {
  return () => {
    const view = graph.getView()
    const { scale } = view
    const t = view.getTranslate()
    const size = GRID * scale
    element.style.backgroundSize = `${size}px ${size}px`
    element.style.backgroundPosition = `${t.x * scale}px ${t.y * scale}px`
    // 太密就變成一片灰，反而看不清楚圖。用 background-image 而不是 opacity：
    // 這個元素是畫布本體，調它的透明度會把整張圖一起弄淡。
    element.style.backgroundImage = size < 5 ? 'none' : ''
  }
}

const icon = (svg: string) => `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`

const CONNECT_ICON = icon(`<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14">
  <circle cx="7" cy="7" r="6" fill="#3aa757" stroke="#fff" stroke-width="1.5"/>
  <path d="M7 4v6M4 7h6" stroke="#fff" stroke-width="1.5" stroke-linecap="round"/>
</svg>`)

const foldIcon = (sign: '+' | '-') => icon(`<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12">
  <rect x="0.5" y="0.5" width="11" height="11" rx="1.5" fill="#fff" stroke="#8a8783"/>
  <path d="${sign === '+' ? 'M6 3v6M3 6h6' : 'M3 6h6'}" stroke="#2b2b2b" stroke-width="1.2" stroke-linecap="round"/>
</svg>`)

/**
 * 拉線。
 *
 * ⚠️ **光開 `setConnectable(true)` 拉不了線。** 連線手勢的觸發點預設是
 * 形狀正中央，而那正好也是「拖曳搬動」的手勢——兩個打架，結果是拖中間
 * 變成拉線、形狀反而搬不動。三個設定疊起來才對：
 *
 * - `connectImage`：給一顆看得見的接點，把兩個手勢分開
 * - `marker.hotspotEnabled = false`：滑到形狀上**任何地方**都算，
 *   預設只有正中央 30%，而使用者不會知道有那個看不見的區域
 * - `getIconPosition`：接點挪到右緣，**但只能在形狀裡面**——挪到外面的話
 *   maxGraph 一發現滑鼠離開 cell 就銷毀它，變成一顆看得到、點不到的按鈕
 */
export function connecting(graph: Graph): void {
  graph.setConnectable(true)
  graph.setAllowDanglingEdges(true)
  graph.setMultigraph(true)

  const handler = graph.getPlugin<ConnectionHandler>('ConnectionHandler')!
  handler.connectImage = new ImageBox(CONNECT_ICON, 14, 14)
  handler.marker.hotspotEnabled = false
  handler.getIconPosition = (icon, state) => {
    const inset = (icon.bounds?.width ?? 14) + 3
    // 太小的形狀塞不下，放中間就好——那時候形狀本來就沒有「邊緣」可言。
    const x = state.width > inset * 2.5 ? state.x + state.width - inset : state.getCenterX()
    return new Point(x, state.getCenterY() - (icon.bounds?.height ?? 14) / 2)
  }

  // 使用者自己拉的線走直角，跟我們產的圖一致。
  const edge = graph.getStylesheet().getDefaultEdgeStyle()
  edge.edgeStyle = 'orthogonalEdgeStyle'
  edge.rounded = false

  // 轉彎點：直角線靠拖整段（`EdgeSegmentHandler`，本來就會），
  // 直線靠這裡打開的 virtual bends。預設關，而且透明度只有 20。
  EdgeHandlerConfig.virtualBendsEnabled = true
  EdgeHandlerConfig.virtualBendOpacity = 55
  EdgeHandlerConfig.addBendOnShiftClickEnabled = true
}

/**
 * 摺疊圖示（容器左上角那個小方塊）。
 *
 * ⚠️ **預設指到 `./collapsed.gif`**，那是 maxGraph 自己的圖檔，而我們沒有
 * 把它放到網站根目錄。結果是 `<image>` 照樣長出來、位置正確、**點得下去**，
 * 但畫不出任何東西——最難查的那一種。用 data URI 自己給。
 */
export function foldIcons(graph: Graph): void {
  graph.options.collapsedImage = new ImageBox(foldIcon('+'), 12, 12)
  graph.options.expandedImage = new ImageBox(foldIcon('-'), 12, 12)
}

/**
 * 復原。
 *
 * ⚠️ `UndoManager` **不會自己聽**圖的變更。少了這三行，工具列上的復原按鈕
 * 安靜地什麼都不做。
 */
export function undoStack(graph: Graph): UndoManager {
  const undoManager = new UndoManager()
  const remember = (_sender: unknown, evt: { getProperty(name: string): unknown }) => {
    undoManager.undoableEditHappened(evt.getProperty('edit') as never)
  }
  graph.getDataModel().addListener(InternalEvent.UNDO, remember)
  graph.getView().addListener(InternalEvent.UNDO, remember)
  return undoManager
}
