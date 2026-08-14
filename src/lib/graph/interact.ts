/**
 * 畫圖的操作：剪貼簿、右鍵選單、鍵盤。
 *
 * 目標是**盡量跟 draw.io 一樣**——使用者的手已經學會那一套了，
 * 每一個不一樣的地方都是一次「咦？」。
 */

import { Clipboard, type Cell, type Graph, type PopupMenuHandler, type UndoManager } from '@maxgraph/core'

/** 圖上的動作對模型是什麼意思——這個檔案回答了其中兩題。 */
export interface Clip {
  copy(): void
  cut(): void
  paste(): void
  duplicate(): void
  copyStyle(): void
  pasteStyle(): void
  hasStyle(): boolean
}

/**
 * 剪貼簿與複製一份。
 *
 * ⚠️ **貼上的東西會帶著原件的 `loomId`**。同一個 `loomId` 出現在兩個形狀上，
 * 對帳就永遠對不起來——所以貼上之後**把綁定清掉**，讓它變成一個
 * 「還沒指定」的形狀。
 *
 * 這是「圖上的動作對模型是什麼意思」的答案之一：**複製一個框，不等於複製
 * 一台機器。**
 */
export function clipboard(graph: Graph, onChange: () => void): Clip {
  /** 複製的是 cell **自己的** style，不是解析後的完整樣式——解析後的那份
   * 混了 stylesheet 的預設值，貼過去會把預設值焊死在形狀上。 */
  let style: Record<string, unknown> | null = null

  const unbind = (cells: Cell[] | null) => {
    graph.batchUpdate(() => {
      for (const cell of cells ?? []) {
        const value = cell.getValue() as unknown
        if (!isElement(value) || !value.hasAttribute('loomId')) continue
        const next = value.cloneNode(true) as Element
        next.removeAttribute('loomId')
        graph.model.setValue(cell, next)
      }
    })
  }

  return {
    copy: () => { Clipboard.copy(graph) },
    cut: () => { Clipboard.cut(graph); onChange() },
    paste: () => { unbind(Clipboard.paste(graph)); onChange() },
    duplicate: () => {
      const cells = graph.getSelectionCells()
      if (!cells.length) return
      graph.batchUpdate(() => {
        // 位移一點，不然貼在原地看起來像沒反應。draw.io 也是這樣。
        const added = graph.addCells(graph.cloneCells(cells), graph.getDefaultParent())
        graph.moveCells(added, 20, 20)
        unbind(added)
        graph.setSelectionCells(added)
      })
      onChange()
    },
    copyStyle: () => {
      const [cell] = graph.getSelectionCells()
      if (cell) style = { ...(cell.getStyle() as Record<string, unknown>) }
    },
    pasteStyle: () => {
      if (!style) return
      graph.batchUpdate(() => {
        for (const cell of graph.getSelectionCells()) graph.model.setStyle(cell, { ...style })
      })
      onChange()
    },
    hasStyle: () => style !== null,
  }
}

function isElement(value: unknown): value is Element {
  return Boolean(value) && typeof value === 'object' && 'getAttribute' in (value as object)
}

/**
 * 右鍵選單。
 *
 * **這是換掉 draw.io 才拿得到的能力**——嵌在 iframe 裡的時候，形狀上按右鍵
 * 出現的是它的選單，我們插不進任何東西（見 `docs/canvas-engine.md`）。
 */
export function contextMenu(
  graph: Graph,
  { clip, onEditData, onChange }: { clip: Clip; onEditData: (cell: Cell) => void; onChange: () => void },
): void {
  const handler = graph.getPlugin<PopupMenuHandler>('PopupMenuHandler')
  if (!handler) return

  handler.factoryMethod = (menu) => {
    const selected = graph.getSelectionCells()
    if (!selected.length) {
      menu.addItem('貼上', null, () => clip.paste())
      menu.addItem('全選', null, () => graph.selectAll())
      return
    }

    menu.addItem('剪下', null, () => clip.cut())
    menu.addItem('複製', null, () => clip.copy())
    menu.addItem('複製一份', null, () => clip.duplicate())
    menu.addSeparator()
    menu.addItem('編輯文字', null, () => graph.startEditingAtCell(selected[0]!))
    menu.addItem('編輯資料…', null, () => onEditData(selected[0]!))
    menu.addSeparator()
    menu.addItem('複製樣式', null, () => clip.copyStyle())
    if (clip.hasStyle()) menu.addItem('貼上樣式', null, () => clip.pasteStyle())
    if (selected.some((c) => c.isEdge())) {
      menu.addItem('拉直（清掉轉彎點）', null, () => {
        graph.batchUpdate(() => {
          for (const cell of selected.filter((c) => c.isEdge())) {
            const geo = cell.getGeometry()?.clone()
            if (!geo) continue
            geo.points = []
            graph.model.setGeometry(cell, geo)
          }
        })
        onChange()
      })
    }
    menu.addSeparator()
    menu.addItem('置前', null, () => { graph.orderCells(false, selected); onChange() })
    menu.addItem('置後', null, () => { graph.orderCells(true, selected); onChange() })
    menu.addSeparator()
    menu.addItem('刪除', null, () => { graph.removeCells(selected); onChange() })
  }
}

/**
 * 鍵盤。
 *
 * ⚠️ **掛在 document，但輸入框有焦點時整組讓開。** ⌘Z 在面板的輸入框裡應該
 * 是「復原我剛打的字」，不是「復原圖上的動作」。掛在 `graph.container` 上
 * 看似安全，實際上焦點在面板時整組快捷鍵就失效了。
 */
export function keyboard(
  graph: Graph,
  undo: UndoManager,
  { clip, onEditData, onChange }: { clip: Clip; onEditData: (cell: Cell) => void; onChange: () => void },
): () => void {
  const onKeydown = (e: KeyboardEvent) => {
    const target = e.target as HTMLElement | null
    const typing = target && ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)
    // 就地編輯標籤時也要讓開，不然打字會觸發刪除。
    if (typing || graph.isEditing()) return

    const meta = e.metaKey || e.ctrlKey
    const [first] = graph.getSelectionCells()

    if (meta && e.key === 'z') e.shiftKey ? undo.redo() : undo.undo()
    // ⚠️ `groupCells` 的型別把 group 標成必填，但實作接受 null（那時它會
    // 自己建一個群組 cell）。`null as never` 是為了不繞過那個實作。
    else if (meta && e.key === 'g') e.shiftKey ? graph.ungroupCells() : graph.groupCells(null as never, 8)
    else if (meta && e.key === 'a') graph.selectAll()
    // ⚠️ 這兩個要看 `code` 不能看 `key`：macOS 上按著 ⌥ 打 C，`key` 是 `ç`。
    else if (meta && e.altKey && e.code === 'KeyC') clip.copyStyle()
    else if (meta && e.altKey && e.code === 'KeyV') clip.pasteStyle()
    else if (meta && e.key === 'c') clip.copy()
    else if (meta && e.key === 'x') clip.cut()
    else if (meta && e.key === 'v') clip.paste()
    else if (meta && e.key === 'd') clip.duplicate()
    else if (meta && e.key === 'm') { if (first) onEditData(first) }
    else if (e.key === 'F2' || e.key === 'Enter') { if (first) graph.startEditingAtCell(first) }
    else if (e.key === 'Backspace' || e.key === 'Delete') graph.removeCells()
    else if (e.key === 'Escape') graph.clearSelection()
    else return

    e.preventDefault()
    onChange()
  }

  document.addEventListener('keydown', onKeydown)
  return () => document.removeEventListener('keydown', onKeydown)
}
