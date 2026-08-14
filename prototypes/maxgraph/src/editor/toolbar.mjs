/**
 * 工具列：最常按的那幾個。
 *
 * # 挑選的標準是「快」
 *
 * 四個目標裡這一排只服務一個：**方便快速**。所以進得了工具列的條件是
 * 「一天會按很多次」，不是「重要」。刪除很重要，但它在鍵盤上；
 * 對齊一天按很多次，所以它在。
 *
 * # 這些動作 maxGraph 幾乎都有
 *
 * `Editor` 那個類別內建 60 個 action（undo/redo/zoom/align/group…），
 * 但它是**吃 XML 設定檔**的（見官方 tutorial），而我們的設定要寫在 TypeScript
 * 裡跟型別一起走。所以這裡直接呼叫 `Graph` 與 `UndoManager` 的 API——
 * 拿到的是同一組能力，少一層 XML。
 */

/** `[標籤, 提示, 動作]`。分隔線用 `null`。 */
export function toolbar(graph, undoManager, { onChange = () => {} } = {}) {
  const el = document.createElement('div')
  el.className = 'toolbar'

  const run = (fn) => () => {
    fn()
    onChange()
  }

  const items = [
    ['↶', '復原 ⌘Z', run(() => undoManager.undo())],
    ['↷', '重做 ⇧⌘Z', run(() => undoManager.redo())],
    null,
    ['＋', '放大', run(() => graph.zoomIn())],
    ['－', '縮小', run(() => graph.zoomOut())],
    ['⤢', '整張圖', run(() => graph.getPlugin('fit').fitCenter({ maxScale: 1.5 }))],
    ['1:1', '實際大小', run(() => graph.zoomActual())],
    null,
    ['⇤', '靠左對齊', run(() => graph.alignCells('left'))],
    ['⇔', '水平置中', run(() => graph.alignCells('center'))],
    ['⇥', '靠右對齊', run(() => graph.alignCells('right'))],
    ['⇧', '靠上對齊', run(() => graph.alignCells('top'))],
    ['⇕', '垂直置中', run(() => graph.alignCells('middle'))],
    ['⇩', '靠下對齊', run(() => graph.alignCells('bottom'))],
    null,
    ['群組', '群組 ⌘G', run(() => graph.groupCells(null, 8))],
    ['解群組', '解群組 ⇧⌘G', run(() => graph.ungroupCells())],
    null,
    ['刪除', '刪除 ⌫', run(() => graph.removeCells())],
  ]

  for (const item of items) {
    if (!item) {
      const line = document.createElement('span')
      line.className = 'sep'
      el.append(line)
      continue
    }
    const [label, title, onClick] = item
    const button = document.createElement('button')
    button.textContent = label
    button.title = title
    button.dataset.action = title.split(' ')[0]
    button.onclick = onClick
    el.append(button)
  }

  return el
}

/**
 * 鍵盤。
 *
 * ⚠️ **不要用 `graph.container` 當監聽對象**：焦點在格式面板的輸入框裡時，
 * ⌘Z 應該是「復原我剛打的字」，不是「復原圖上的動作」。所以掛在 document 上，
 * 但**輸入框有焦點時整個讓開**。
 */
export function keyboard(graph, undoManager, { onChange = () => {} } = {}) {
  const onKeydown = (e) => {
    const typing = e.target instanceof HTMLElement
      && ['INPUT', 'TEXTAREA', 'SELECT'].includes(e.target.tagName)
    if (typing) return

    const meta = e.metaKey || e.ctrlKey
    if (meta && e.key === 'z') {
      e.shiftKey ? undoManager.redo() : undoManager.undo()
    } else if (meta && e.key === 'g') {
      e.shiftKey ? graph.ungroupCells() : graph.groupCells(null, 8)
    } else if (meta && e.key === 'a') {
      graph.selectAll()
    } else if (e.key === 'Backspace' || e.key === 'Delete') {
      graph.removeCells()
    } else if (e.key === 'Escape') {
      graph.clearSelection()
    } else {
      return
    }
    e.preventDefault()
    onChange()
  }

  document.addEventListener('keydown', onKeydown)
  return () => document.removeEventListener('keydown', onKeydown)
}
