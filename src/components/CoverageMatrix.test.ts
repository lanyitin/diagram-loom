/**
 * 覆蓋矩陣的渲染測試。
 *
 * 這一層只驗**畫面有沒有照實把 Rust 給的東西畫出來**。
 * 「什麼叫缺漏」是 Rust 的事，那邊有 `tests/coverage_matrix.rs` 顧著。
 *
 * 所以這裡刻意用手寫的假 snapshot：如果哪天前端偷偷自己判斷狀態，
 * 這些測試會因為「我給了 realized 但畫出未實現」而紅燈。
 */

import { describe, expect, it, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import CoverageMatrix from './CoverageMatrix.vue'
import { useProject } from '../lib/store'
import type { Cell, Snapshot, Status } from '../lib/model'

function cell(
  relationship: string,
  environment: string,
  status: Status,
  extra: Partial<Cell> = {},
): Cell {
  return {
    relationship,
    environment,
    status,
    segments: 1,
    targets: 1,
    expect: null,
    rules: [],
    ...extra,
  }
}

/** 兩條契約 × 三個環境，涵蓋四種狀態。 */
function fakeSnapshot(): Snapshot {
  const cells: Cell[] = [
    cell('r-cache', 'env-prod', 'realized', { segments: 2, targets: 3, expect: 3 }),
    cell('r-cache', 'env-test', 'warning', { segments: 2, targets: 2, expect: null, rules: ['L005'] }),
    cell('r-cache', 'env-dev', 'realized'),
    cell('r-pay', 'env-prod', 'broken', { segments: 2, targets: 3, expect: 4, rules: ['L004'] }),
    cell('r-pay', 'env-test', 'realized'),
    cell('r-pay', 'env-dev', 'missing', { segments: 0, targets: 0, rules: ['L001'] }),
  ]

  return {
    root: '/tmp/假的.loom',
    findings: [],
    matrix: {
      relationships: ['r-cache', 'r-pay'],
      environments: ['env-prod', 'env-test', 'env-dev'],
      cells,
    },
    project: {
      id: 'p',
      slug: 'fake',
      name: '假專案',
      logical: {
        people: [],
        systems: [],
        containers: [],
        relationships: [
          { id: 'r-cache', slug: 'api-連-redis', purpose: '讀寫快取', from: { container: 'c' }, to: { container: 'c' }, toEndpoint: 'e' },
          { id: 'r-pay', slug: 'api-連-金流', purpose: '送出付款', from: { container: 'c' }, to: { container: 'c' }, toEndpoint: 'e' },
        ],
      },
      environments: [
        { id: 'env-prod', slug: 'prod', name: '正式' },
        { id: 'env-test', slug: 'test', name: '測試' },
        { id: 'env-dev', slug: 'dev', name: '開發' },
      ],
    },
  } as unknown as Snapshot
}

/**
 * 環境那幾欄。前兩欄固定是契約與用途，其餘就是環境。
 *
 * 欄名列由共用的 `TableHead` 畫，所以不再有 `.env` 這個 class——
 * 依位置取的理由是欄的**組成**本來就由這裡定義：契約、用途、然後每個環境一欄。
 */
function envColumns(w: ReturnType<typeof mount>) {
  return w.findAll('thead th[data-column]').slice(2).map((t) => t.text())
}

describe('覆蓋矩陣', () => {
  let store: ReturnType<typeof useProject>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.snapshot = fakeSnapshot()
  })

  it('每條契約一列，每個環境一欄', () => {
    const w = mount(CoverageMatrix)
    expect(w.findAll('tbody tr')).toHaveLength(2)
    expect(envColumns(w)).toEqual(['prod', 'test', 'dev'])
  })

  it('點環境那一欄，最糟的排前面', async () => {
    // 點某個環境那一欄的人要找的就是「這個環境還缺什麼」，
    // 不是照字母看熱鬧。跟連線表的「實際／期望」同一個約定。
    const w = mount(CoverageMatrix)
    const contracts = () => w.findAll('tbody tr').map((r) => r.findAll('td')[0]!.text())
    const before = contracts()

    await w.findAll('thead th[data-column]').at(-1)!.trigger('click')

    const after = contracts()
    expect(after).toHaveLength(before.length)
    expect(new Set(after)).toEqual(new Set(before))
    // 第一列在 dev 那一欄的狀態，要是所有列裡最糟的。
    const worst = w.findAll('tbody tr')[0]!.findAll('.chip').at(-1)!
    expect(worst.classes()).toContain('missing')
  })

  it('矩陣不放「欄位」選單', () => {
    // 它的欄就是環境，而工具列上的環境勾選已經是那個開關了。
    // 同一件事有兩個入口的話，使用者兩個都不會信。
    expect(mount(CoverageMatrix).find('thead .cols').exists()).toBe(false)
  })

  it('缺漏的格子寫「未實現」而且不是實心的', () => {
    const w = mount(CoverageMatrix)
    const missing = w.find('.chip.missing')
    expect(missing.text()).toBe('未實現')
    // 狀態不能只靠顏色：缺漏用虛線框表示「空的」，這個 class 就是那個訊號。
    expect(missing.classes()).toContain('missing')
  })

  it('格子顯示幾段幾台，不是打勾', () => {
    // 打勾只回答「有沒有」；prod 走 F5 兩段、dev 直連一段的差別才是會咬人的地方。
    const w = mount(CoverageMatrix)
    const firstCell = w.findAll('tbody tr')[0]!.findAll('.chip')
    expect(firstCell[0]!.text()).toBe('2 段 · 3 台')
    expect(firstCell[2]!.text()).toBe('1 段')
  })

  it('數量對不上時把實際與期望並排', () => {
    // 萬用字元最危險的地方是「少一台看起來完全正常」，所以兩個數字要貼在一起。
    const w = mount(CoverageMatrix)
    expect(w.find('.chip.broken').text()).toBe('2 段 · 3／4 台')
  })

  it('狀態完全照 Rust 給的畫，前端不自己判斷', () => {
    // 給一個「零段但標成 realized」的矛盾資料。前端如果偷偷自己判斷，
    // 就會畫成「未實現」——那代表規則漏到前端了。
    store.snapshot!.matrix.cells[2] = cell('r-cache', 'env-dev', 'realized', { segments: 0, targets: 0 })
    const w = mount(CoverageMatrix)
    const firstCell = w.findAll('tbody tr')[0]!.findAll('.chip')
    expect(firstCell[2]!.classes()).toContain('realized')
  })

  it('勾選環境之後只剩勾起來的欄位', () => {
    store.selectedEnvironments = ['env-prod', 'env-dev']
    const w = mount(CoverageMatrix)
    expect(envColumns(w)).toEqual(['prod', 'dev'])
  })

  it('只看有問題時會濾掉全綠的列', () => {
    store.snapshot!.matrix.cells = store.snapshot!.matrix.cells.map((c) =>
      c.relationship === 'r-cache' ? { ...c, status: 'realized' as Status, rules: [] } : c,
    )
    store.onlyProblems = true
    const w = mount(CoverageMatrix)
    expect(w.findAll('tbody tr')).toHaveLength(1)
    expect(w.find('tbody .rel').text()).toBe('api-連-金流')
  })

  it('搜尋會同時比對名稱與用途', () => {
    store.search = '付款'
    const w = mount(CoverageMatrix)
    expect(w.findAll('tbody tr')).toHaveLength(1)
    expect(w.find('tbody .rel').text()).toBe('api-連-金流')
  })

  it('篩到沒有東西時給一句話，而不是一片空白', () => {
    store.search = '不存在的東西'
    const w = mount(CoverageMatrix)
    expect(w.find('.empty').text()).toContain('沒有符合條件')
  })
})
