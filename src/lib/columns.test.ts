/**
 * 欄位偏好。
 *
 * 這裡真正在守的是**存欄名不是存索引**。那個坑不會讓任何東西壞掉，
 * 只會讓使用者藏錯欄位——而他不會把這回報成 bug，只會覺得
 * 「這個工具怪怪的」。
 */

import { beforeEach, describe, expect, it } from 'vitest'
import { ref } from 'vue'
import { load, MIN_WIDTH, save, useColumns } from './columns'

const KEY = 'container'

beforeEach(() => localStorage.clear())

describe('要顯示哪些欄', () => {
  it('預設全部顯示', () => {
    const { visible } = useColumns(ref(KEY))
    expect(visible(['名稱', '顯示名', '用途'])).toEqual(['名稱', '顯示名', '用途'])
  })

  it('關掉一欄之後它就不畫了', () => {
    const { visible, toggle } = useColumns(ref(KEY))
    toggle('顯示名')
    expect(visible(['名稱', '顯示名', '用途'])).toEqual(['名稱', '用途'])
  })

  it('第一欄關不掉', () => {
    // 那是這一列的名字。關掉之後就不知道自己在看誰了。
    const { visible, toggle } = useColumns(ref(KEY))
    toggle('名稱')
    expect(visible(['名稱', '顯示名'])).toContain('名稱')
  })

  it('沒見過的欄一律顯示', () => {
    // 欄位是 Rust 給的。它多給一欄的時候應該要看得到，
    // 而不是因為沒存過就默默不見。
    save(KEY, { hidden: ['顯示名'], widths: {} })
    const { visible } = useColumns(ref(KEY))
    expect(visible(['名稱', '顯示名', '全新的欄'])).toEqual(['名稱', '全新的欄'])
  })
})

describe('存的是欄名不是第幾欄', () => {
  it('中間插一欄之後，藏起來的還是同一欄', () => {
    // 「服務」與「契約」兩張表每多一個環境就多一欄。存索引的話，
    // 同事加一個環境、你重開 App，藏起來的欄位就整排錯位。
    const { visible, toggle } = useColumns(ref(KEY))
    toggle('用途')

    // 同事加了一個環境，它插在「用途」前面。
    expect(visible(['名稱', '顯示名', 'staging', '用途'])).toEqual(['名稱', '顯示名', 'staging'])
  })

  it('重開之後記得', () => {
    const key = ref(KEY)
    useColumns(key).toggle('用途')
    expect(load(KEY).hidden).toEqual(['用途'])

    // 「重開 App」＝重新建一份。
    expect(useColumns(ref(KEY)).isHidden('用途')).toBe(true)
  })

  it('換一張表就換一份偏好', () => {
    const key = ref(KEY)
    const columns = useColumns(key)
    columns.toggle('用途')

    key.value = 'node'
    expect(columns.isHidden('用途')).toBe(false)

    key.value = KEY
    expect(columns.isHidden('用途')).toBe(true)
  })
})

describe('欄寬', () => {
  it('拖過的欄記下來，沒拖過的不記', () => {
    // 沒拖過的欄要一直跟著內容走。把量出來的值也存起來的話，
    // 資料變了欄寬也不會跟著變——那個舊值會一直蓋在上面。
    const { prefs, setWidth } = useColumns(ref(KEY))
    setWidth('用途', 240)

    expect(prefs.value.widths).toEqual({ 用途: 240 })
    expect(load(KEY).widths).toEqual({ 用途: 240 })
  })

  it('再窄也有個底', () => {
    const { prefs, setWidth } = useColumns(ref(KEY))
    setWidth('用途', -50)
    expect(prefs.value.widths['用途']).toBe(MIN_WIDTH)
  })

  it('雙擊之後那一欄還給內容', () => {
    const { prefs, setWidth, clearWidth } = useColumns(ref(KEY))
    setWidth('用途', 240)
    clearWidth('用途')
    expect(prefs.value.widths).toEqual({})
  })
})

describe('存壞了不要讓畫面掛掉', () => {
  it('讀到不是 JSON 的東西就當作沒存過', () => {
    localStorage.setItem('diagram-loom.columns.' + KEY, '{壞掉的')
    expect(load(KEY)).toEqual({ hidden: [], widths: {} })
  })

  it('讀到形狀不對的東西也一樣', () => {
    localStorage.setItem('diagram-loom.columns.' + KEY, '{"hidden":"不是陣列"}')
    expect(load(KEY)).toEqual({ hidden: [], widths: {} })
  })
})
