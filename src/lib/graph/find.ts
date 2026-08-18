/**
 * 在圖上找形狀。
 *
 * # 為什麼比對的是**圖上的文字**，不是模型裡的名字
 *
 * 使用者搜尋的是他**看得到**的字。App 自動產的那張圖，形狀的標籤本來就是
 * 從模型印出來的，所以兩者一致；而使用者自己上傳、自己標的圖上，那些字
 * 是他寫的——那時候「模型裡叫什麼」根本不是他手上的資訊。
 *
 * # 為什麼比對的規矩要跟表格一模一樣
 *
 * 表格的搜尋是 `trim().toLowerCase().includes()`（見 `lib/rows.ts` 與
 * `store` 的 `visibleRows`）。同一個工具裡兩種搜尋語意，使用者只會覺得
 * 「有時候找得到有時候找不到」，而且他無從得知差別在哪。
 *
 * 所以這裡**刻意不支援萬用字元**，即使連線的兩端用的是 `redis-*` 那一套。
 * 那一套是拿來**指涉一群東西**的，不是拿來搜尋的。
 */

import type { UnboundShape } from '../model'

/**
 * 圖上符合這段文字的形狀，照圖上的順序。
 *
 * 空字串回空陣列，不是「全部」——搜尋框沒打字時不該有一個「目前這一個」。
 */
export function matching(shapes: UnboundShape[], keyword: string): UnboundShape[] {
  const needle = keyword.trim().toLowerCase()
  if (!needle) return []
  return shapes.filter((s) => s.label.toLowerCase().includes(needle))
}

/**
 * 走到下一個（或上一個），會繞回去。
 *
 * 繞回去是刻意的：一直按「下一個」走到底卻沒反應，使用者會以為卡住了。
 * `total` 是 0 時回 0——呼叫端不必先檢查，而且那時候本來就沒有東西可指。
 */
export function step(current: number, total: number, delta: number): number {
  if (total <= 0) return 0
  return (current + delta + total) % total
}
