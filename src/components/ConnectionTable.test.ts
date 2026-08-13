/**
 * 連線表與 Lint 面板的渲染測試。
 *
 * 一樣只驗「有沒有照實把 Rust 給的東西畫出來」。名稱解析與萬用字元展開
 * 由 `tests/connection_table.rs` 顧著。
 */

import { beforeEach, describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import ConnectionTable from './ConnectionTable.vue'
import LintPanel from './LintPanel.vue'
import { useProject } from '../lib/store'
import type { Finding, Row, Side, Snapshot } from '../lib/model'

function 端(label: string, extra: Partial<Side> = {}): Side {
  return { kind: 'instance', label, endpoint: null, addresses: [], matched: 1, expect: null, ...extra }
}

function 列(id: string, extra: Partial<Row> = {}): Row {
  return {
    id,
    environment: 'env-prod',
    serves: 'r-cache',
    servesSlug: 'api-連-redis',
    purpose: '讀寫快取',
    from: 端('app-01'),
    to: 端('redis-*', { endpoint: 'client-port', addresses: ['10.0.1.11:6379', '10.0.1.12:6379', '10.0.1.13:6379'], matched: 3, expect: 3 }),
    kind: 'primary',
    severity: null,
    rules: [],
    ...extra,
  }
}

function 假快照(rows: Row[], findings: Finding[] = []): Snapshot {
  return {
    root: '/tmp/假的.loom',
    findings,
    rows,
    matrix: { relationships: [], environments: [], cells: [] },
    project: {
      id: 'p', slug: 'f', name: '假專案',
      logical: { people: [], systems: [], containers: [], relationships: [] },
      environments: [
        { id: 'env-prod', slug: 'prod', name: '正式' },
        { id: 'env-dev', slug: 'dev', name: '開發' },
      ],
    },
  } as unknown as Snapshot
}

describe('連線表', () => {
  let store: ReturnType<typeof useProject>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.snapshot = 假快照([列('c1')])
  })

  /** 依表頭名稱取儲存格，不用位置——加一欄就全錯的測試沒有價值。 */
  function 欄位(w: ReturnType<typeof mount>, 表頭: string) {
    const i = w.findAll('thead th').findIndex((t) => t.text() === 表頭)
    expect(i, `找不到「${表頭}」這一欄`).toBeGreaterThanOrEqual(0)
    return w.findAll('tbody tr')[0]!.findAll('td')[i]!
  }

  it('端點顯示成「名稱 : 接點」', () => {
    const w = mount(ConnectionTable)
    expect(欄位(w, '來源').text()).toBe('app-01')
    expect(欄位(w, '目標').text()).toBe('redis-* : client-port')
  })

  it('位址很多時只顯示第一個加數量', () => {
    // 十幾個位址塞進一格會把表格撐爛，而使用者要的是「大概在哪一段」。
    const w = mount(ConnectionTable)
    expect(欄位(w, '目標位址').text()).toBe('10.0.1.11:6379 +2')
  })

  it('數量對不上時把數字本身標起來', () => {
    store.snapshot = 假快照([列('c1', { to: 端('redis-*', { matched: 3, expect: 4 }) })])
    const w = mount(ConnectionTable)
    expect(w.find('.對不上').exists()).toBe(true)
    expect(w.find('.對不上').text()).toBe('3／4')
  })

  it('數量相符時不標記', () => {
    const w = mount(ConnectionTable)
    expect(w.find('.對不上').exists()).toBe(false)
  })

  it('沒有任何列用得到期望數量時整欄不出現', () => {
    // 畫面上不放永遠空白的欄位。
    store.snapshot = 假快照([列('c1', { to: 端('redis-01') })])
    const w = mount(ConnectionTable)
    expect(w.findAll('thead th').map((t) => t.text())).not.toContain('實際／期望')
  })

  it('搜尋比對得到位址', () => {
    // 「這個 IP 是誰在用」是實際會發生的問題。
    store.搜尋 = '10.0.1.12'
    const w = mount(ConnectionTable)
    expect(w.findAll('tbody tr')).toHaveLength(1)

    store.搜尋 = '10.9.9.9'
    expect(mount(ConnectionTable).find('.empty').exists()).toBe(true)
  })

  it('備援路徑會標出來', () => {
    // 四條線一樣重的話，讀的人分不出平常的資料流是哪幾條。
    store.snapshot = 假快照([列('c1'), 列('c2', { kind: 'fallback' })])
    const w = mount(ConnectionTable)
    expect(w.findAll('.fb')).toHaveLength(1)
    expect(w.findAll('tbody tr.fallback')).toHaveLength(1)
  })

  it('只看有問題會濾掉沒問題的列', () => {
    store.snapshot = 假快照([列('c1'), 列('c2', { severity: 'error', rules: ['L004'] })])
    store.只看有問題 = true
    const w = mount(ConnectionTable)
    expect(w.findAll('tbody tr')).toHaveLength(1)
  })
})

describe('Lint 面板', () => {
  let store: ReturnType<typeof useProject>

  const 發現: Finding[] = [
    { rule: 'L004', severity: 'error', environment: 'env-prod', subject: 'conn-1', detail: '期望 4 個，實際 3 個' },
    { rule: 'L007', severity: 'warning', environment: 'env-dev', subject: 'conn-2', detail: '連線沒有填用途' },
  ]

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.snapshot = 假快照([列('c1')], 發現)
  })

  it('收合時只有一條摘要列', () => {
    const w = mount(LintPanel)
    expect(w.find('.list').exists()).toBe(false)
    expect(w.find('.bar').text()).toContain('1 錯誤')
    expect(w.find('.bar').text()).toContain('1 警告')
  })

  it('展開後列出每一項', () => {
    store.面板展開 = true
    const w = mount(LintPanel)
    expect(w.findAll('.list tbody tr')).toHaveLength(2)
  })

  it('點一項會跳到那個環境的連線表並只留有問題的列', () => {
    // 「知道有錯」到「看到那一列」之間不該需要自己找。
    store.面板展開 = true
    const w = mount(LintPanel)
    w.findAll('.list tbody tr')[1]!.trigger('click')

    expect(store.檢視).toBe('連線表')
    expect(store.只看有問題).toBe(true)
    expect(store.比對中的環境).toEqual(['env-dev'])
    expect(store.搜尋).toBe('')
  })

  it('沒問題時說一句話而不是空清單', () => {
    store.snapshot = 假快照([列('c1')], [])
    store.面板展開 = true
    const w = mount(LintPanel)
    expect(w.find('.bar').text()).toContain('沒有發現問題')
    expect(w.find('.empty').text()).toContain('沒有任何缺漏')
  })
})
