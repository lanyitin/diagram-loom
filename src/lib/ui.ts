/**
 * 介面縮放：小／中／大。
 *
 * # 為什麼不放進專案檔
 *
 * 這是**這台機器上這個人**的偏好，不是專案的內容。專案資料夾會被
 * 分享、進版控，把「我看不清楚小字」寫進去只會在別人的機器上造成疑惑。
 *
 * 所以走 `localStorage`，也因此不需要經過 Rust——它不是規則，是顯示設定。
 *
 * # 為什麼縮放整個介面
 *
 * 見 `styles.css` 裡 `--zoom` 的說明：只放大字會讓密度很高的表格擠在一起，
 * 而且核取方塊與小按鈕不會跟著變大。
 */

export type Scale = 'small' | 'medium' | 'large'

/**
 * 三個級距。
 *
 * 最小的那一級是 `1`，也就是**原本的大小**——加這個功能不該讓
 * 已經習慣現狀的人被迫改變。
 */
export const SCALES: { value: Scale; label: string; zoom: number }[] = [
  { value: 'small', label: '小', zoom: 1 },
  { value: 'medium', label: '中', zoom: 1.15 },
  { value: 'large', label: '大', zoom: 1.32 },
]

const KEY = 'diagram-loom.ui-scale'

export function load(): Scale {
  const saved = localStorage.getItem(KEY)
  return SCALES.some((s) => s.value === saved) ? (saved as Scale) : 'small'
}

/** 套用到畫面，並記下來。下次開起來要是同一個大小。 */
export function apply(scale: Scale) {
  const zoom = SCALES.find((s) => s.value === scale)?.zoom ?? 1
  document.documentElement.style.setProperty('--zoom', String(zoom))
  localStorage.setItem(KEY, scale)
}
