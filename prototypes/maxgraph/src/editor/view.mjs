/**
 * 畫布的操作手感：平移、縮放、格線。
 *
 * # 為什麼這件事要特別做
 *
 * `new Graph(container)` 給的是一張**不會動的**畫布。`setPanning(true)` 也
 * 只是開了 handler，預設要**右鍵**拖曳才平移，滾輪則完全沒有反應。
 * 這跟 draw.io 的手感差很遠，而「手感」正是使用者第一秒就會發現的東西。
 *
 * | 動作 | 做什麼 |
 * | --- | --- |
 * | 空白處拖曳 | 平移 |
 * | 滾輪 | **以游標為中心縮放** |
 * | ⇧ + 滾輪 | 左右平移 |
 * | ⌘／Ctrl + 滾輪 | 也是縮放（手記得這個的人不會撲空） |
 *
 * ⚠️ **滾輪的預設是縮放，不是捲動**，這一條跟 draw.io 的網頁版不一樣。
 * 第一版照 draw.io 做成「滾輪捲動、⌘＋滾輪縮放」，結果使用者的回報是
 * 「沒有 zoom in/out」——他滾了滾輪，畫面只是平移。
 *
 * 這裡的畫布是**無限大的**：沒有頁面邊界，捲動沒有盡頭也沒有參考點，
 * 而平移已經有「拖空白處」這個更直接的手勢了。所以滾輪讓給縮放。
 */

import { ImageBox, InternalEvent } from '@maxgraph/core'

/** 格線的間距。draw.io 預設也是 10。 */
const GRID = 10

/** 摺疊圖示：方框加一個 ＋／－，跟 draw.io 的形狀接近。 */
const foldIcon = (sign) => `data:image/svg+xml;utf8,${encodeURIComponent(
  `<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12">
     <rect x="0.5" y="0.5" width="11" height="11" rx="1.5" fill="#fff" stroke="#8a8783"/>
     <path d="${sign === '+' ? 'M6 3v6M3 6h6' : 'M3 6h6'}" stroke="#2b2b2b" stroke-width="1.2" stroke-linecap="round"/>
   </svg>`,
)}`

/**
 * 摺疊圖示（容器左上角那個小方塊）。
 *
 * ⚠️ **maxGraph 預設指到 `./collapsed.gif` 與 `./expanded.gif`**——
 * 那是它自己的圖檔，而我們沒有把它們放到網站根目錄。結果是
 * `<image>` 元素照樣長出來、位置正確、**點得下去**，但畫不出任何東西。
 *
 * 這是最難查的那種：功能好好的，只是看不見。而且 vite 會把找不到的路徑
 * 回成 `index.html`（200），連 404 都沒有。
 *
 * 所以自己給圖，用 data URI——不依賴任何檔案放在哪裡，打包進 Tauri 也一樣。
 */
export function foldIcons(graph) {
  graph.options.collapsedImage = new ImageBox(foldIcon('+'), 12, 12)
  graph.options.expandedImage = new ImageBox(foldIcon('-'), 12, 12)
}

export function panAndZoom(graph, { onView = () => {} } = {}) {
  graph.setPanning(true)

  const panning = graph.getPlugin('PanningHandler')
  // 空白處按著左鍵就能平移；按在形狀上仍然是搬形狀（`ignoreCell` 留 false）。
  panning.useLeftButtonForPanning = true
  panning.ignoreCell = false
  // 右鍵留給選單，不要拿去平移。
  panning.usePopupTrigger = false

  // 對齊格線。少了它，手動拖出來的東西永遠差幾個像素，
  // 而「容易閱讀」有一半就是對齊。
  graph.setGridEnabled(true)
  graph.gridSize = GRID

  InternalEvent.addMouseWheelListener((evt, up) => {
    if (!graph.container.contains(evt.target)) return

    if (evt.shiftKey) {
      const view = graph.getView()
      const t = view.getTranslate()
      // 除以 scale：放大之後同樣的滾輪格數應該走一樣的**螢幕**距離。
      const step = 48 / view.scale
      view.setTranslate(t.x + (up ? step : -step), t.y)
    } else {
      zoomAt(graph, evt, up)
    }
    // 不擋掉的話整個頁面會跟著捲，畫布看起來像「卡住又亂跳」。
    evt.preventDefault()
    onView()
  }, graph.container)

  // 縮放與平移都要讓背景格線跟著走，否則格線會跟形狀脫鉤——
  // 那比沒有格線更糟，因為它會騙人。
  const view = graph.getView()
  view.addListener(InternalEvent.SCALE, onView)
  view.addListener(InternalEvent.TRANSLATE, onView)
  view.addListener(InternalEvent.SCALE_AND_TRANSLATE, onView)
}

/**
 * 以游標為中心縮放。
 *
 * `graph.zoomIn()` 是**以畫布中心**縮放的，用滾輪放大時你盯著的那個東西
 * 會滑走。要讓游標底下那個點不動：先算出它在圖座標上的位置，縮放後把
 * 平移補回去。
 */
function zoomAt(graph, evt, up) {
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
 * maxGraph **沒有內建可見的格線**（`setGridEnabled` 只管吸附）。用 CSS 畫，
 * 然後跟著 view 的縮放與平移更新——這樣它才會跟形狀一起動。
 *
 * ⚠️ **格線一定要是畫布自己的 `background`，不能是一層蓋上去的 div。**
 * 蓋上去的那個 div 是 `absolute`、maxGraph 的 `<svg>` 是 `static`，
 * 而**定位過的元素會畫在未定位的內容之上，跟 DOM 順序無關**——
 * 結果是格線壓在圖上面，穿過每一個形狀跟每一行字。
 */
export function grid(graph, element) {
  return () => {
    const view = graph.getView()
    const { scale } = view
    const t = view.getTranslate()
    const size = GRID * scale
    element.style.backgroundSize = `${size}px ${size}px`
    element.style.backgroundPosition = `${t.x * scale}px ${t.y * scale}px`
    // 格線太密就變成一片灰，反而看不清楚圖。draw.io 也是縮小到某個程度就收掉。
    // 用 `background-image` 而不是 `opacity`：這個元素現在是畫布本體，
    // 調它的透明度會把整張圖一起弄淡。
    element.style.backgroundImage = size < 5 ? 'none' : ''
  }
}
