/**
 * 形狀庫：使用者可以自己拖出來的東西。
 *
 * # 為什麼有這個
 *
 * 圖是從模型產生的，所以一開始的判斷是「不該讓使用者拖形狀」——拖出來的
 * 東西模型不認得。但**使用者要能自由畫圖**，所以做了，而那個顧慮沒有消失，
 * 只是換了答案：
 *
 * > **拖出來的形狀不帶 `loomId`，所以它會自動出現在「還沒指定」那份清單裡，
 * > 並且被算進「還剩幾個」。**
 *
 * 畫圖是自由的，而「圖上有一個模型不認得的東西」不會被藏起來。
 *
 * # draw.io 的形狀只帶一小包
 *
 * `vendor/drawio` 有 1561 個形狀、42 MB 且不進版控。畫一張架構圖用得到的
 * 只有幾十個，所以挑了 33 個進版控（`mise run stencils`）。
 * 保留 draw.io 的命名，樣式字串因此跟它通用——使用者上傳的圖用到這些形狀時
 * 我們畫得出來。
 */

import { StencilShape, StencilShapeRegistry } from '@maxgraph/core'

import { STYLE } from '../diagram'

/** 一格形狀庫。`style` 用 draw.io 的字串寫法，跟我們產圖時同一套。 */
export interface Palette {
  label: string
  width: number
  height: number
  style: string
}

/**
 * 我們自己的模型形狀。放第一組，因為那是這個工具最常畫的東西。
 *
 * 樣式直接引用 `STYLE`——**跟產圖時是同一份**。抄一份的話，使用者手拖出來的
 * 機器跟自動產出來的機器會長得不一樣，而那個差別會被當成「有意義的區別」。
 */
export const MODEL_SHAPES: Palette[] = [
  { label: '站點', width: 240, height: 160, style: STYLE.site },
  { label: '機器', width: 200, height: 120, style: STYLE.node },
  { label: '服務', width: 160, height: 60, style: STYLE.instance },
  { label: '設備', width: 160, height: 60, style: STYLE.infra },
  { label: '外部系統', width: 160, height: 60, style: STYLE.system },
  { label: '人', width: 40, height: 70, style: STYLE.person },
]

/** 通用形狀。畫框、分區、註記用得到。 */
export const BASIC_SHAPES: Palette[] = [
  { label: '方框', width: 120, height: 60, style: 'whiteSpace=wrap;html=1;' },
  { label: '圓角', width: 120, height: 60, style: 'rounded=1;whiteSpace=wrap;html=1;' },
  { label: '橢圓', width: 120, height: 60, style: 'ellipse;whiteSpace=wrap;html=1;' },
  { label: '菱形', width: 100, height: 100, style: 'rhombus;whiteSpace=wrap;html=1;' },
  { label: '圓柱', width: 80, height: 100, style: 'shape=cylinder;whiteSpace=wrap;html=1;' },
  { label: '雲', width: 120, height: 80, style: 'ellipse;shape=cloud;whiteSpace=wrap;html=1;' },
  { label: '便利貼', width: 100, height: 80, style: 'shape=note;whiteSpace=wrap;html=1;fillColor=#fff2cc;' },
  // 無框的純文字。真實的架構圖上到處都是——區塊說明、圖例、線旁邊的註記。
  { label: '文字', width: 120, height: 30, style: 'text;html=1;fillColor=none;strokeColor=none;align=left;' },
  // 分隔線。那張真實架構圖有四條垂直虛線，用很窄的虛線框做。
  { label: '分隔線', width: 1, height: 400, style: 'fillColor=none;dashed=1;strokeColor=#8a8783;' },
]

/** draw.io 的形狀。名字照它的規矩：`<shapes name>` + `<shape name>`，小寫、空白換底線。 */
export const NETWORK_SHAPES: Palette[] = [
  ['Server', '伺服器', 80, 100],
  ['Firewall', '防火牆', 100, 60],
  ['Switch', '交換器', 100, 60],
  ['Router', '路由器', 80, 80],
  ['Load Balancer', '負載平衡', 80, 100],
  ['Storage', '儲存', 80, 100],
  ['Cloud', '網際網路', 120, 70],
  ['Users', '使用者', 80, 60],
  ['Mobile', '行動裝置', 40, 70],
  ['Mainframe', '主機', 80, 100],
].map(([name, label, width, height]) => ({
  label: label as string,
  width: width as number,
  height: height as number,
  style: `shape=mxgraph.networks.${String(name).toLowerCase().replace(/ /g, '_')};html=1;`,
}))

/**
 * 把帶進版控的 stencil 註冊進去。
 *
 * ⚠️ **查不到 stencil 時 maxGraph 會默默退回矩形**，標籤照樣在。所以
 * 「有畫出東西」不能當成功的判準——回傳註冊了幾個，讓呼叫端說得出實話。
 */
export function registerStencils(xml: string): number {
  const doc = new DOMParser().parseFromString(xml, 'text/xml')
  const root = doc.documentElement
  if (root.tagName !== 'shapes') return 0

  const pack = root.getAttribute('name') ?? ''
  let count = 0
  for (const el of Array.from(root.children)) {
    if (el.tagName !== 'shape') continue
    const name = `${pack}.${el.getAttribute('name')}`.toLowerCase().replace(/ /g, '_')
    StencilShapeRegistry.add(name, new StencilShape(el))
    count += 1
  }
  return count
}
