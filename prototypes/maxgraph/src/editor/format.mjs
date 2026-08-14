/**
 * 格式面板：draw.io 右邊那一排東西。
 *
 * # 為什麼非有不可
 *
 * 換掉 draw.io 之後，**這一整塊是我們自己要長出來的**。使用者的原話是
 * 「原本的那些樣式修改功能都必須要有」，而它服務的是四個目標裡的
 * **容易閱讀**——一張圖要進簡報，顏色、字級、對齊就是可讀性本身。
 *
 * # 三段，跟著選取變
 *
 * 選了框給「形狀 / 文字 / 幾何」，選了線給「線」，什麼都沒選就整片收起來。
 * draw.io 是分頁（Style / Text / Arrange），這裡改成**一次攤開**——
 * 我們的面板窄、控制項少，分頁只會多一次點擊。
 *
 * # 每個控制項都直接打 maxGraph 的 API
 *
 * `setCellStyles` / `toggleCellStyleFlags` / `alignCells` / `resizeCell`。
 * 這是這個原型要回答的問題之一：**接一個格式面板要付多少**。
 * 答案就是這個檔案的長度。
 */

import { actions, color, group, number, row, select, toggle } from './widgets.mjs'

/** 粗體 1、斜體 2、底線 4。`FONT_STYLE_MASK` 有定義但 `index.js` 沒有 re-export。 */
const FONT = { BOLD: 1, ITALIC: 2, UNDERLINE: 4 }

const FONTS = [
  ['Helvetica', 'Helvetica'],
  ['Arial', 'Arial'],
  ['Georgia', 'Georgia'],
  ['Courier New', 'Courier New'],
  ['Noto Sans TC', 'Noto Sans TC'],
]

/** 線怎麼走。第一個是我們產圖時用的。 */
const EDGE_STYLES = [
  ['orthogonalEdgeStyle', '直角'],
  ['none', '直線'],
  ['elbowEdgeStyle', '折線'],
  ['entityRelationEdgeStyle', '實體關係'],
]

const ARROWS = [
  ['classic', '實心箭頭'],
  ['open', '開放箭頭'],
  ['block', '方塊'],
  ['diamond', '菱形'],
  ['oval', '圓'],
  ['none', '無'],
]

/**
 * 建一片格式面板。
 *
 * `onChange` 在每次改完樣式之後叫一次——外面拿它去更新狀態列與「未存檔」。
 */
export function formatPanel(graph, { onChange = () => {} } = {}) {
  const el = document.createElement('div')
  el.className = 'format'

  /** 現在選了什麼。面板不自己記，每次選取改變重讀一次。 */
  let cells = []

  /** 改樣式的共同入口。**一定要包成一筆**，否則復原要按很多次。 */
  const style = (key, value) => {
    graph.batchUpdate(() => graph.setCellStyles(key, value, cells))
    onChange()
  }
  const flag = (key, bit, on) => {
    graph.batchUpdate(() => graph.setCellStyleFlags(key, bit, on, cells))
    onChange()
  }
  const act = (fn) => () => {
    graph.batchUpdate(() => fn())
    onChange()
    refresh()
  }

  // ── 形狀 ────────────────────────────────────────────────────
  const fill = color((v) => style('fillColor', v))
  const stroke = color((v) => style('strokeColor', v), { none: false })
  const strokeWidth = number((v) => style('strokeWidth', v), { min: 0, max: 24, step: 0.5 })
  const dashed = toggle('虛線', (on) => style('dashed', on))
  const rounded = toggle('圓角', (on) => style('rounded', on))
  const shadow = toggle('陰影', (on) => style('shadow', on))
  // 透明度是**這個 App 自己在用的**（篩選調暗），所以面板上要看得到目前值，
  // 不然使用者調過之後會跟我們的螢光筆打架，而且看不出是誰改的。
  const opacity = number((v) => style('opacity', v), { min: 0, max: 100, step: 5 })

  const shapeGroup = group(
    '形狀',
    row('填色', fill),
    row('框線', stroke, strokeWidth),
    row('樣式', dashed, rounded, shadow),
    row('透明度', opacity),
  )

  // ── 文字 ────────────────────────────────────────────────────
  const fontFamily = select(FONTS, (v) => style('fontFamily', v))
  const fontSize = number((v) => style('fontSize', v), { min: 6, max: 72 })
  const fontColor = color((v) => style('fontColor', v), { none: false })
  const bold = toggle('B', (on) => flag('fontStyle', FONT.BOLD, on), '粗體')
  const italic = toggle('I', (on) => flag('fontStyle', FONT.ITALIC, on), '斜體')
  const underline = toggle('U', (on) => flag('fontStyle', FONT.UNDERLINE, on), '底線')
  const align = select([['left', '靠左'], ['center', '置中'], ['right', '靠右']], (v) =>
    style('align', v))
  const verticalAlign = select([['top', '靠上'], ['middle', '置中'], ['bottom', '靠下']], (v) =>
    style('verticalAlign', v))

  const textGroup = group(
    '文字',
    row('字型', fontFamily),
    row('大小', fontSize, fontColor),
    row('樣式', bold, italic, underline),
    row('對齊', align, verticalAlign),
  )

  // ── 幾何 ────────────────────────────────────────────────────
  //
  // 打字改座標**不是可有可無的**：拖曳永遠對不齊，而「容易閱讀」有一半
  // 就是對齊。draw.io 的 Arrange 分頁也是這樣。
  const geometry = {
    x: number((v) => move('x', v), { min: -9999 }),
    y: number((v) => move('y', v), { min: -9999 }),
    width: number((v) => resize('width', v), { min: 1 }),
    height: number((v) => resize('height', v), { min: 1 }),
  }

  function move(axis, value) {
    graph.batchUpdate(() => {
      for (const cell of cells) {
        const geo = cell.getGeometry()?.clone()
        if (!geo) continue
        geo[axis] = value
        graph.model.setGeometry(cell, geo)
      }
    })
    onChange()
  }

  function resize(axis, value) {
    graph.batchUpdate(() => {
      for (const cell of cells) {
        const geo = cell.getGeometry()?.clone()
        if (!geo) continue
        geo[axis] = value
        graph.model.setGeometry(cell, geo)
      }
    })
    onChange()
  }

  const geometryGroup = group(
    '位置與大小',
    row('位置', geometry.x, geometry.y),
    row('大小', geometry.width, geometry.height),
  )

  // ── 線 ──────────────────────────────────────────────────────
  const edgeStyle = select(EDGE_STYLES, (v) => style('edgeStyle', v === 'none' ? null : v))
  const startArrow = select(ARROWS, (v) => style('startArrow', v))
  const endArrow = select(ARROWS, (v) => style('endArrow', v))
  const clearBends = actions([[
    '拉直',
    act(() => {
      for (const cell of cells) {
        const geo = cell.getGeometry()?.clone()
        if (!geo) continue
        geo.points = []
        graph.model.setGeometry(cell, geo)
      }
    }),
    '清掉轉彎點',
  ]])

  const edgeGroup = group(
    '線',
    row('走法', edgeStyle),
    row('箭頭', startArrow, endArrow),
    row('轉彎點', clearBends),
  )

  // ── 排列 ────────────────────────────────────────────────────
  //
  // ⚠️ maxGraph 有 `alignCells`，**但沒有「分佈」**（draw.io 有）。
  // 等距排開要自己算，見 `distribute`。
  const alignActions = actions([
    ['⇤', act(() => graph.alignCells('left', cells)), '靠左對齊'],
    ['⇔', act(() => graph.alignCells('center', cells)), '水平置中'],
    ['⇥', act(() => graph.alignCells('right', cells)), '靠右對齊'],
    ['⇧', act(() => graph.alignCells('top', cells)), '靠上對齊'],
    ['⇕', act(() => graph.alignCells('middle', cells)), '垂直置中'],
    ['⇩', act(() => graph.alignCells('bottom', cells)), '靠下對齊'],
  ])

  const distributeActions = actions([
    ['⋯', act(() => distribute('x', 'width')), '水平等距'],
    ['⋮', act(() => distribute('y', 'height')), '垂直等距'],
  ])

  /** 等距排開。maxGraph 沒有這個，所以自己算。 */
  function distribute(axis, size) {
    if (cells.length < 3) return
    const boxes = cells
      .map((cell) => ({ cell, geo: cell.getGeometry()?.clone() }))
      .filter((b) => b.geo)
      .sort((a, b) => a.geo[axis] - b.geo[axis])
    const first = boxes[0].geo[axis]
    const last = boxes[boxes.length - 1].geo[axis] + boxes[boxes.length - 1].geo[size]
    const used = boxes.reduce((n, b) => n + b.geo[size], 0)
    const gap = (last - first - used) / (boxes.length - 1)

    let at = first
    for (const box of boxes) {
      box.geo[axis] = at
      at += box.geo[size] + gap
      graph.model.setGeometry(box.cell, box.geo)
    }
  }

  const orderActions = actions([
    ['置前', act(() => graph.orderCells(false, cells)), '移到最上層'],
    ['置後', act(() => graph.orderCells(true, cells)), '移到最下層'],
  ])

  const arrangeGroup = group(
    '排列',
    row('對齊', alignActions),
    row('分佈', distributeActions),
    row('層次', orderActions),
  )

  const empty = document.createElement('p')
  empty.className = 'empty'
  empty.textContent = '選一個形狀或一條線。'

  el.append(empty, shapeGroup, textGroup, geometryGroup, edgeGroup, arrangeGroup)

  /**
   * 把面板灌成目前選取的樣子。
   *
   * **多選時顯示第一個的值**。draw.io 的做法是留空表示「值不一樣」，
   * 那比較誠實，但要每個控制項多一種狀態——原型先不做，改成在標題上標數量，
   * 讓使用者至少知道自己在改好幾個。
   */
  function refresh() {
    cells = graph.getSelectionCells()
    const vertices = cells.filter((c) => c.isVertex())
    const edges = cells.filter((c) => c.isEdge())

    empty.hidden = cells.length > 0
    shapeGroup.hidden = vertices.length === 0
    textGroup.hidden = cells.length === 0
    geometryGroup.hidden = vertices.length !== 1
    edgeGroup.hidden = edges.length === 0
    arrangeGroup.hidden = cells.length < 2

    if (!cells.length) return

    const style = graph.getCellStyle(cells[0])
    fill.set(style.fillColor)
    stroke.set(style.strokeColor)
    strokeWidth.set(style.strokeWidth ?? 1)
    dashed.set(style.dashed)
    rounded.set(style.rounded)
    shadow.set(style.shadow)
    opacity.set(style.opacity ?? 100)

    fontFamily.set(style.fontFamily ?? 'Helvetica')
    fontSize.set(style.fontSize ?? 12)
    fontColor.set(style.fontColor ?? '#000000')
    const fontStyle = style.fontStyle ?? 0
    bold.set(fontStyle & FONT.BOLD)
    italic.set(fontStyle & FONT.ITALIC)
    underline.set(fontStyle & FONT.UNDERLINE)
    align.set(style.align ?? 'center')
    verticalAlign.set(style.verticalAlign ?? 'middle')

    if (vertices.length === 1) {
      const geo = vertices[0].getGeometry()
      geometry.x.set(Math.round(geo?.x ?? 0))
      geometry.y.set(Math.round(geo?.y ?? 0))
      geometry.width.set(Math.round(geo?.width ?? 0))
      geometry.height.set(Math.round(geo?.height ?? 0))
    }

    if (edges.length) {
      edgeStyle.set(style.edgeStyle ?? 'none')
      startArrow.set(style.startArrow ?? 'none')
      endArrow.set(style.endArrow ?? 'classic')
    }
  }

  return { el, refresh }
}
