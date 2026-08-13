/**
 * 資源表的排序與篩選。
 *
 * # 為什麼不是一行 `.sort()` 跟一行 `.filter()`
 *
 * **機器那張表是一棵樹。** 它是攤平的列，靠 `depth` 表示「站點 → 機器 → 機器」
 * 的層級，畫面用縮排畫出來。
 *
 * 所以：
 *
 * - **直接排序會把樹拆散**——子節點跑到別的父節點底下，而縮排還在，
 *   於是畫面會顯示一個**錯的**從屬關係。那比不能排序糟得多。
 * - **直接篩選會讓子節點變孤兒**——父節點被篩掉、子節點留著，縮排就在說謊。
 *
 * 兩件事的解法都是同一句話：**先把樹重建起來，再動手。**
 *
 * 父節點就是「往前找，第一個 `depth` 少一階的那列」——這是攤平樹的定義，
 * 所以不需要 Rust 那邊多給一個 `parent` 欄位。
 */

import type { ResourceRow } from './model'

/** 排序的方向。`null` 表示照 Rust 給的原始順序。 */
export type Direction = 'asc' | 'desc'

/**
 * 哪一欄、哪個方向。
 *
 * **一律用欄名認，不用第幾欄。**
 *
 * 連線表本來就是這樣，因為它的「實際／期望」欄會依內容出現或消失，
 * 用位置會排到別欄去，而且看起來像排錯了而不是抓錯欄。
 *
 * 資源表原本用第幾欄，在欄位還不能藏的時候沒問題。一旦使用者可以關掉
 * 某幾欄，「畫面上第 3 欄」跟「`cells` 裡第 3 格」就不是同一欄了——
 * 同一個坑，所以兩張表用同一個解法。
 */
export interface Sort<C = string> {
  column: C
  direction: Direction
}

interface Node {
  row: ResourceRow
  children: Node[]
}

/**
 * 把攤平的列重建成樹。
 *
 * 全部 `depth` 都是 0 的表（大多數）就是一層 N 個節點，後面的處理完全一樣。
 */
function toTree(rows: ResourceRow[]): Node[] {
  const roots: Node[] = []
  // stack[d] 是目前這條路徑上深度 d 的那個節點。
  const stack: Node[] = []

  for (const row of rows) {
    const node: Node = { row, children: [] }
    const depth = row.depth ?? 0
    stack.length = depth
    const parent = stack[depth - 1]
    if (parent) parent.children.push(node)
    else roots.push(node)
    stack[depth] = node
  }

  return roots
}

function flatten(nodes: Node[], out: ResourceRow[] = []): ResourceRow[] {
  for (const n of nodes) {
    out.push(n.row)
    flatten(n.children, out)
  }
  return out
}

/**
 * 依某一欄排序，**只在兄弟之間排**。
 *
 * 用 `localeCompare` 的 `numeric`：不然 `vm-10` 會排在 `vm-2` 前面，
 * 而機器名字幾乎都是這種帶編號的。
 */
export function sortRows(
  rows: ResourceRow[],
  sort: Sort | null,
  columns: string[],
): ResourceRow[] {
  if (!sort) return rows

  // 欄名 → `cells` 裡的第幾格。這個轉換只在這裡做一次，呼叫端不必知道
  // 資料的排法——那正是欄位可以藏起來之後最容易搞錯的地方。
  const at = columns.indexOf(sort.column)
  if (at < 0) return rows

  const sign = sort.direction === 'asc' ? 1 : -1
  const compare = (a: Node, b: Node) => {
    const x = a.row.cells[at] ?? ''
    const y = b.row.cells[at] ?? ''
    return sign * x.localeCompare(y, undefined, { numeric: true, sensitivity: 'base' })
  }

  const walk = (nodes: Node[]): Node[] => {
    const sorted = [...nodes].sort(compare)
    for (const n of sorted) n.children = walk(n.children)
    return sorted
  }

  return flatten(walk(toTree(rows)))
}

/**
 * 關鍵字篩選。樹的兩個方向都要留：
 *
 * - **往上**：子節點對得上時父節點要留著。不留的話子節點會變孤兒——
 *   縮排還在，但它縮排在誰底下已經沒人知道了。
 * - **往下**：父節點對得上時整棵子樹要留著。搜一個站點的名字，
 *   想看到的就是「這個站點裡有什麼」，只剩一個空框沒有意義。
 */
export function filterRows(rows: ResourceRow[], keyword: string): ResourceRow[] {
  const needle = keyword.trim().toLowerCase()
  if (!needle) return rows

  const hit = (row: ResourceRow) =>
    row.cells.some((c) => c.toLowerCase().includes(needle))

  const keep = (nodes: Node[]): Node[] =>
    nodes.flatMap((n) => {
      if (hit(n.row)) return [n]
      const children = keep(n.children)
      return children.length ? [{ ...n, children }] : []
    })

  return flatten(keep(toTree(rows)))
}

/**
 * 點同一欄就換方向，點別欄就從遞增開始。第三次點回到原始順序。
 *
 * **一定要回得到原始順序**：Rust 給的順序是有意義的（機器是樹、
 * 契約照定義順序），排過就回不去的話那個資訊就沒了。
 */
export function nextSort<C>(current: Sort<C> | null, column: C): Sort<C> | null {
  if (current?.column !== column) return { column, direction: 'asc' }
  if (current.direction === 'asc') return { column, direction: 'desc' }
  return null
}


/**
 * 一般的排序：一張平的表，照某一欄的值排。
 *
 * 給連線表用——它沒有樹，但點標題的行為要跟資源表一模一樣，
 * 所以共用同一個 [`nextSort`]，而不是各寫一套。
 *
 * 值是數字就照數字比。字串則用 `numeric` 比對，不然 `vm-10` 會排在 `vm-2` 前面。
 */
export function sortBy<T, C extends string | number>(
  items: T[],
  sort: Sort<C> | null,
  key: (item: T, column: C) => string | number,
): T[] {
  if (!sort) return items
  const sign = sort.direction === 'asc' ? 1 : -1
  return [...items].sort((a, b) => {
    const x = key(a, sort.column)
    const y = key(b, sort.column)
    if (typeof x === 'number' && typeof y === 'number') return sign * (x - y)
    return sign * String(x).localeCompare(String(y), undefined, {
      numeric: true,
      sensitivity: 'base',
    })
  })
}
