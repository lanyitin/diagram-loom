/**
 * 畫圖的操作：連線、剪貼簿、右鍵選單。
 *
 * 目標是**盡量跟 draw.io 一樣**——使用者的手已經學會那一套了，
 * 每一個不一樣的地方都是一次「咦？」。
 */

import { Clipboard, ImageBox, InternalEvent, Point } from '@maxgraph/core'

/**
 * 拉線。
 *
 * 這是 draw.io「畫圖」體驗的核心：滑到形狀上會出現一顆接點，拖出去就是一條線。
 * maxGraph 的 `ConnectionHandler` 內建這件事，但**光把 `connectable` 打開
 * 是不夠的**，見下面那個 ⚠️。
 *
 * ⚠️ **允許連到空白處**（dangling），因為 draw.io 允許，而使用者的手記得
 * 這件事。代價是那條線在模型上沒有意義——它會跟自由拖出來的形狀一樣，
 * 落在「還沒指定」那份清單裡。
 */
const CONNECT_ICON = `data:image/svg+xml;utf8,${encodeURIComponent(
  `<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14">
     <circle cx="7" cy="7" r="6" fill="#3aa757" stroke="#fff" stroke-width="1.5"/>
     <path d="M7 4v6M4 7h6" stroke="#fff" stroke-width="1.5" stroke-linecap="round"/>
   </svg>`,
)}`

export function connecting(graph) {
  graph.setConnectable(true)
  graph.setAllowDanglingEdges(true)
  graph.setMultigraph(true)

  // ⚠️ **一定要給一個 connectImage。**
  //
  // 不給的話，maxGraph 的連線手勢是「滑到形狀**中間**再拖曳」——而那正好
  // 也是搬動形狀的手勢，兩個會打架（實測：拖曳中間變成拉線，搬不動形狀）。
  //
  // draw.io 的解法是在形狀邊上放箭頭，拖箭頭＝拉線、拖形狀＝搬動。這裡放
  // 一顆小圖示是同一個道理：**把兩個手勢分開**。
  const handler = graph.getPlugin('ConnectionHandler')
  handler.connectImage = new ImageBox(CONNECT_ICON, 14, 14)

  // ⚠️ 預設只有**形狀正中央 30%** 會讓接點出現（`DEFAULT_HOTSPOT = 0.3`），
  // 而使用者不會知道有那個看不見的區域——他滑過形狀邊緣，什麼都沒發生，
  // 於是結論是「不能自己拉線」。關掉 hotspot：**滑到形狀上任何地方都算**。
  handler.marker.hotspotEnabled = false

  // 接點預設畫在**形狀正中央**，而那正好是使用者拖曳搬動形狀的地方——
  // 按下去到底是拉線還是搬動，變成看你有沒有壓到那顆 14px 的點。
  // 所以把它挪到右緣。
  //
  // ⚠️ **但只能挪到形狀「裡面」的右緣，不能挪到外面。**
  //
  // maxGraph 一發現滑鼠離開這個 cell 就把接點銷毀（`mouseMove` 裡那句
  // `previous !== currentState → destroyIcons()`），而且**沒有「滑鼠在接點上
  // 就留著」的例外**。放到形狀外面的話，使用者往它移動的途中接點就消失了，
  // 變成一顆**看得到、點不到**的按鈕——比沒有更糟。
  //
  // draw.io 的箭頭在形狀外面，是因為它自己寫了那段 hover 容許區。
  handler.getIconPosition = (icon, state) => {
    const inset = icon.bounds.width + 3
    // 太小的形狀塞不下，放中間就好——那時候形狀本來就沒有「邊緣」可言。
    const x = state.width > inset * 2.5 ? state.x + state.width - inset : state.getCenterX()
    return new Point(x, state.getCenterY() - icon.bounds.height / 2)
  }
  // 使用者自己拉的線走直角，跟我們產的圖一致。
  graph.getStylesheet().getDefaultEdgeStyle().edgeStyle = 'orthogonalEdgeStyle'
  graph.getStylesheet().getDefaultEdgeStyle().rounded = false
}

/**
 * 剪貼簿與複製一份。
 *
 * ⚠️ **貼上的東西會帶著原件的自訂屬性**，包含 `loomId`。同一個 `loomId`
 * 出現在兩個形狀上，對帳就永遠對不起來——所以貼上之後**把綁定清掉**，
 * 讓它變成一個「還沒指定」的形狀。
 *
 * 這是「圖上的動作對模型是什麼意思」的第一個答案：**複製一個框，不等於
 * 複製一台機器。**
 */
export function clipboard(graph, { onChange = () => {} } = {}) {
  const unbind = (cells) => {
    graph.batchUpdate(() => {
      for (const cell of cells ?? []) {
        const value = cell.getValue()
        if (value?.nodeType !== 1) continue
        const next = value.cloneNode(true)
        next.removeAttribute('loomId')
        graph.model.setValue(cell, next)
      }
    })
    return cells
  }

  return {
    copy: () => Clipboard.copy(graph),
    cut: () => { Clipboard.cut(graph); onChange() },
    paste: () => { unbind(Clipboard.paste(graph)); onChange() },
    duplicate: () => {
      const cells = graph.getSelectionCells()
      if (!cells.length) return
      // 位移一點，不然貼在原地看起來像沒反應。draw.io 也是這樣。
      const copies = graph.cloneCells(cells)
      graph.batchUpdate(() => {
        const added = graph.addCells(copies, graph.getDefaultParent())
        graph.moveCells(added, 20, 20)
        unbind(added)
        graph.setSelectionCells(added)
      })
      onChange()
    },
  }
}

/**
 * 右鍵選單。
 *
 * **這是換掉 draw.io 才拿得到的能力**——嵌在 iframe 裡的時候，形狀上按右鍵
 * 出現的是它的選單，我們插不進任何東西（見 `docs/canvas-engine.md`）。
 */
export function contextMenu(graph, { clip, onEditData, onChange = () => {} }) {
  const handler = graph.getPlugin('PopupMenuHandler')
  handler.factoryMethod = (menu, cell) => {
    const selected = graph.getSelectionCells()
    const has = selected.length > 0

    if (has) {
      menu.addItem('剪下', null, () => { clip.cut(); onChange() })
      menu.addItem('複製', null, () => clip.copy())
      menu.addItem('複製一份', null, () => clip.duplicate())
      menu.addSeparator()
      menu.addItem('編輯文字', null, () => graph.startEditingAtCell(selected[0]))
      menu.addItem('編輯資料…', null, () => onEditData(selected[0]))
      menu.addSeparator()
      menu.addItem('置前', null, () => { graph.orderCells(false, selected); onChange() })
      menu.addItem('置後', null, () => { graph.orderCells(true, selected); onChange() })
      menu.addSeparator()
      menu.addItem('刪除', null, () => { graph.removeCells(selected); onChange() })
    } else {
      menu.addItem('貼上', null, () => clip.paste())
      menu.addItem('全選', null, () => graph.selectAll())
    }
    // `cell` 目前只用來決定有沒有東西被點到；選單本身跟著「選取」走，
    // 因為使用者右鍵之前通常已經選好了一批。
    void cell
  }
}

/**
 * 雙擊空白處新增一個方框。
 *
 * draw.io 有這個，而且是最快的新增方式——不必去左邊找形狀。
 */
export function doubleClickToInsert(graph, insert) {
  graph.addListener(InternalEvent.DOUBLE_CLICK, (_sender, evt) => {
    if (evt.getProperty('cell')) return
    const mouse = evt.getProperty('event')
    const point = graph.getPointForEvent(mouse)
    insert({ label: '方框', w: 120, h: 60, style: {} }, point.x, point.y)
    evt.consume()
  })
}
