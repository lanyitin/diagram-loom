/**
 * 連線表與 Lint 面板的渲染測試。
 *
 * 一樣只驗「有沒有照實把 Rust 給的東西畫出來」。名稱解析與萬用字元展開
 * 由 `tests/connection_table.rs` 顧著。
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
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
    subjects: [id, 'r-cache'],
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
    {
      rule: 'L004', severity: 'error', environment: 'env-prod', subject: 'conn-1',
      detail: '期望 4 個，實際 3 個',
      fix: { count: { suggestion: 3 } },
    },
    {
      rule: 'L007', severity: 'warning', environment: 'env-dev', subject: 'conn-2',
      detail: '連線沒有填用途',
      fix: { text: { hint: '這條連線是做什麼用的', current: null } },
    },
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

  it('點一項會跳到那個環境的連線表，並聚焦到那一項本身', () => {
    // 「知道有錯」到「看到那一列」之間不該需要自己找。
    store.面板展開 = true
    const w = mount(LintPanel)
    w.findAll('.list tbody .detail')[1]!.trigger('click')

    expect(store.檢視).toBe('連線表')
    expect(store.比對中的環境).toEqual(['env-dev'])
    expect(store.搜尋).toBe('')
    expect(store.聚焦?.subject).toBe('conn-2')
  })

  it('連點兩項不同的發現，第二次也要有反應', () => {
    // 這是真的踩過的：原本只切到「那個環境的有問題的列」，
    // 同一個環境裡連點兩項，畫面一模一樣，看起來就像第二次點壞掉。
    store.面板展開 = true
    const w = mount(LintPanel)

    w.findAll('.list tbody .detail')[0]!.trigger('click')
    const 第一次 = store.聚焦?.subject

    w.findAll('.list tbody .detail')[1]!.trigger('click')
    expect(store.聚焦?.subject).not.toBe(第一次)
  })

  it('聚焦時只留跟那一項有關的列', () => {
    store.snapshot = 假快照(
      [列('c1'), 列('c2', { subjects: ['c2', 'r-cache'] })],
      發現,
    )
    store.聚焦 = { subject: 'c2', label: 'x' }
    expect(store.顯示的列.map((r) => r.id)).toEqual(['c2'])
  })

  it('聚焦到沒有對應列的東西時，得到的是空清單而不是全部', () => {
    // 「篩不到就顯示全部」是最糟的：使用者以為那一項牽涉到每一條連線。
    store.snapshot = 假快照([列('c1')], 發現)
    store.聚焦 = { subject: '邏輯層的東西', label: 'x' }
    expect(store.顯示的列).toEqual([])
  })

  it('修法的控制項完全由 Rust 送來的 fix 決定', async () => {
    // 前端不認得規則代號。同一個面板，L004 長出數字框、L007 長出文字框，
    // 差別只在 `fix` 的形狀——這裡若壞掉，代表有人在前端加了規則對照表。
    store.面板展開 = true
    const w = mount(LintPanel)

    await w.findAll('.list .fix')[0]!.trigger('click')
    expect(w.find('.editor input').attributes('type')).toBe('number')
    // 實際符合幾個是 Rust 算的，直接當預設值，不叫使用者自己數。
    expect((w.find('.editor input').element as HTMLInputElement).value).toBe('3')

    await w.findAll('.list .fix')[1]!.trigger('click')
    expect(w.find('.editor input').attributes('type')).toBe('text')
    expect(w.find('.editor input').attributes('placeholder')).toBe('這條連線是做什麼用的')
  })

  it('填完送出的是發現本身加上值，不是前端拼的 Edit', async () => {
    store.面板展開 = true
    const 修好 = vi.spyOn(store, '修好').mockResolvedValue(undefined)
    const w = mount(LintPanel)

    await w.findAll('.list .fix')[1]!.trigger('click')
    await w.find('.editor input').setValue('查快取')
    await w.find('.editor form').trigger('submit')

    expect(修好).toHaveBeenCalledWith(發現[1], { text: '查快取' })
  })

  it('數字那種修法送出的是數字', async () => {
    // 真的踩過：`<input type="number">` 的 v-model 會自動把值轉成數字，
    // 送出時對它呼叫 .trim() 直接炸掉，按下「套用」完全沒反應。
    // 只驗控制項長得對是不夠的——每一種修法都要真的送出一次。
    store.面板展開 = true
    const 修好 = vi.spyOn(store, '修好').mockResolvedValue(undefined)
    const w = mount(LintPanel)

    await w.findAll('.list .fix')[0]!.trigger('click')
    await w.find('.editor input').setValue('4')
    await w.find('.editor form').trigger('submit')

    expect(修好).toHaveBeenCalledWith(發現[0], { count: 4 })
  })

  it('數字留空表示拿掉期望數量，不是不動它', async () => {
    store.面板展開 = true
    const 修好 = vi.spyOn(store, '修好').mockResolvedValue(undefined)
    const w = mount(LintPanel)

    await w.findAll('.list .fix')[0]!.trigger('click')
    await w.find('.editor input').setValue('')
    await w.find('.editor form').trigger('submit')

    expect(修好).toHaveBeenCalledWith(發現[0], { count: null })
  })

  it('開關那種修法不需要輸入框', async () => {
    store.snapshot = 假快照([列('c1')], [
      {
        rule: 'L008', severity: 'warning', environment: 'env-prod', subject: 'i-1',
        detail: '沒人碰', fix: { toggle: { label: '刻意獨立（冷備機等）' } },
      },
    ])
    store.面板展開 = true
    const 修好 = vi.spyOn(store, '修好').mockResolvedValue(undefined)
    const w = mount(LintPanel)

    await w.find('.list .fix').trigger('click')
    expect(w.find('.editor input').exists()).toBe(false)
    expect(w.find('.editor').text()).toContain('刻意獨立')

    await w.find('.editor form').trigger('submit')
    expect(修好).toHaveBeenCalledWith(expect.objectContaining({ rule: 'L008' }), { toggle: true })
  })

  it('沒有單欄位修法的那幾條不放假按鈕', () => {
    // 按下去只會說「這個還沒做」的按鈕，比沒有按鈕更糟。
    store.snapshot = 假快照([列('c1')], [
      { rule: 'L002', severity: 'error', environment: 'env-prod', subject: 'r-1', detail: '走不通', fix: null },
    ])
    store.面板展開 = true
    const w = mount(LintPanel)

    expect(w.find('.list .fix').exists()).toBe(false)
    expect(w.find('.list .act').text()).toContain('要改連線')
  })

  it('沒問題時說一句話而不是空清單', () => {
    store.snapshot = 假快照([列('c1')], [])
    store.面板展開 = true
    const w = mount(LintPanel)
    expect(w.find('.bar').text()).toContain('沒有發現問題')
    expect(w.find('.empty').text()).toContain('沒有任何缺漏')
  })
})
