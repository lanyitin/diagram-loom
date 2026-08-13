/**
 * 表格的欄寬與「要顯示哪些欄」。
 *
 * # 為什麼走 localStorage，不進專案資料夾
 *
 * 跟介面大小（`ui.ts`）同一個理由：這是**這台機器上這個人**的偏好，
 * 不是專案的內容。專案資料夾會被分享、進版控，把「我不想看用途那欄」
 * 寫進去只會在同事的機器上造成疑惑。
 *
 * 所以它也不需要經過 Rust——它不是規則，是顯示設定。
 *
 * # ⚠️ 存的是欄「名」，不是第幾欄
 *
 * 這是最容易踩的一個坑。「服務」與「契約」兩張表**每多一個環境就多一欄**
 * （`inventory.rs` 裡的 `columns.extend(project.environments…)`）。
 *
 * 存索引的話，同事加一個環境、你重開 App，所有藏起來的欄位就全部錯位——
 * 而畫面看起來完全正常，只是藏錯了東西。這種錯誤沒有人會回報成 bug，
 * 只會變成「這個工具怪怪的」。
 *
 * # 寬度分兩種，只有一種要記
 *
 * - **量出來的**：第一次畫完之後照內容量的，放在元件裡，不留下來。
 * - **拖出來的**：使用者自己拉的，記到 localStorage。
 *
 * 兩種混在一起記的話，資料變了（多了一台機器、位址變長）欄寬也不會跟著變，
 * 因為那個舊的量測值一直蓋在上面。分開之後，沒動過的欄永遠跟著內容走，
 * 動過的欄則完全聽使用者的。
 */

import { ref, watch, type Ref } from 'vue'

/** 一張表的欄位偏好。 */
export interface ColumnPrefs {
  /** 藏起來的欄名。 */
  hidden: string[]
  /** 欄名 → 使用者拖出來的寬度（px）。沒拖過的欄不在裡面。 */
  widths: Record<string, number>
}

/** 欄再窄就看不到內容了，拖曳到此為止。 */
export const MIN_WIDTH = 56

const PREFIX = 'diagram-loom.columns.'

function empty(): ColumnPrefs {
  return { hidden: [], widths: {} }
}

export function load(key: string): ColumnPrefs {
  try {
    const raw = localStorage.getItem(PREFIX + key)
    if (!raw) return empty()
    const parsed = JSON.parse(raw) as Partial<ColumnPrefs>
    return {
      hidden: Array.isArray(parsed.hidden) ? parsed.hidden.filter((c) => typeof c === 'string') : [],
      widths: typeof parsed.widths === 'object' && parsed.widths ? parsed.widths : {},
    }
  } catch {
    // 存壞了就當作沒存過。這是顯示設定，壞掉不值得讓畫面掛掉。
    return empty()
  }
}

export function save(key: string, prefs: ColumnPrefs): void {
  try {
    localStorage.setItem(PREFIX + key, JSON.stringify(prefs))
  } catch {
    // 無痕視窗、配額滿了之類。記不起來不是錯誤，下次重來就好。
  }
}

/**
 * 綁在某一張表上的欄位偏好。
 *
 * `key` 是響應式的：資源檢視換一個分頁就是換一張表，偏好要跟著換。
 * 用 `kind` 而不是分頁標題當 key——標題是給人看的字，之後改個用詞
 * 就會把使用者的設定弄丟。
 */
export function useColumns(key: Ref<string>) {
  const prefs = ref<ColumnPrefs>(load(key.value))

  /*
   * 兩個都是 `flush: 'sync'`。
   *
   * 預設的 `'pre'` 是下一個 tick 才跑，而換分頁時晚一拍的後果是**畫面會用
   * 上一張表的偏好先畫一次**——欄名列會拿舊的「藏起來的欄」去對新的欄位，
   * 而 `TableHead` 就在那一拍量欄寬，量到的是一組不存在的欄。
   */
  watch(key, (k) => { prefs.value = load(k) }, { flush: 'sync' })
  watch(prefs, (p) => save(key.value, p), { deep: true, flush: 'sync' })

  /**
   * 這張表要畫哪幾欄。
   *
   * **第一欄永遠留著**：那是這一列的名字，關掉之後就不知道自己在看誰了。
   * 不認得的欄名一律當作要顯示——欄位是 Rust 給的，它多給一欄的時候
   * 應該要看得到，而不是因為沒存過就默默不見。
   */
  function visible(columns: string[]): string[] {
    return columns.filter((c, i) => i === 0 || !prefs.value.hidden.includes(c))
  }

  function isHidden(column: string): boolean {
    return prefs.value.hidden.includes(column)
  }

  function toggle(column: string): void {
    const hidden = new Set(prefs.value.hidden)
    if (hidden.has(column)) hidden.delete(column)
    else hidden.add(column)
    prefs.value = { ...prefs.value, hidden: [...hidden] }
  }

  function setWidth(column: string, px: number): void {
    prefs.value = {
      ...prefs.value,
      widths: { ...prefs.value.widths, [column]: Math.max(MIN_WIDTH, Math.round(px)) },
    }
  }

  /** 把某一欄還給內容——雙擊拖曳把手就是這個。 */
  function clearWidth(column: string): void {
    const widths = { ...prefs.value.widths }
    delete widths[column]
    prefs.value = { ...prefs.value, widths }
  }

  return { prefs, visible, isHidden, toggle, setWidth, clearWidth }
}
