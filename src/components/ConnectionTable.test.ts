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

function side(label: string, extra: Partial<Side> = {}): Side {
  return { kind: 'instance', label, endpoint: null, addresses: [], matched: 1, expect: null, ...extra }
}

function rows(id: string, extra: Partial<Row> = {}): Row {
  return {
    id,
    environment: 'env-prod',
    serves: 'r-cache',
    servesSlug: 'api-連-redis',
    purpose: '讀寫快取',
    from: side('app-01'),
    to: side('redis-*', { endpoint: 'client-port', addresses: ['10.0.1.11:6379', '10.0.1.12:6379', '10.0.1.13:6379'], matched: 3, expect: 3 }),
    kind: 'primary',
    severity: null,
    rules: [],
    subjects: [id, 'r-cache'],
    ...extra,
  }
}

function fakeSnapshot(rows: Row[], findings: Finding[] = []): Snapshot {
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
    store.snapshot = fakeSnapshot([rows('c1')])
  })

  /** 依表頭名稱取儲存格，不用位置——加一欄就全錯的測試沒有價值。 */
  function columnCell(w: ReturnType<typeof mount>, headers: string) {
    const i = w.findAll('thead th').findIndex((t) => t.text() === headers)
    expect(i, `找不到「${headers}」這一欄`).toBeGreaterThanOrEqual(0)
    return w.findAll('tbody tr')[0]!.findAll('td')[i]!
  }

  it('端點顯示成「名稱 : 接點」', () => {
    const w = mount(ConnectionTable)
    expect(columnCell(w, '來源').text()).toBe('app-01')
    expect(columnCell(w, '目標').text()).toBe('redis-* : client-port')
  })

  it('位址很多時只顯示第一個加數量', () => {
    // 十幾個位址塞進一格會把表格撐爛，而使用者要的是「大概在哪一段」。
    const w = mount(ConnectionTable)
    expect(columnCell(w, '目標位址').text()).toBe('10.0.1.11:6379 +2')
  })

  it('數量對不上時把數字本身標起來', () => {
    store.snapshot = fakeSnapshot([rows('c1', { to: side('redis-*', { matched: 3, expect: 4 }) })])
    const w = mount(ConnectionTable)
    expect(w.find('.mismatch').exists()).toBe(true)
    expect(w.find('.mismatch').text()).toBe('3／4')
  })

  it('數量相符時不標記', () => {
    const w = mount(ConnectionTable)
    expect(w.find('.mismatch').exists()).toBe(false)
  })

  it('沒有任何列用得到期望數量時整欄不出現', () => {
    // 畫面上不放永遠空白的欄位。
    store.snapshot = fakeSnapshot([rows('c1', { to: side('redis-01') })])
    const w = mount(ConnectionTable)
    expect(w.findAll('thead th').map((t) => t.text())).not.toContain('實際／期望')
  })

  it('搜尋比對得到位址', () => {
    // 「這個 IP 是誰在用」是實際會發生的問題。
    store.search = '10.0.1.12'
    const w = mount(ConnectionTable)
    expect(w.findAll('tbody tr')).toHaveLength(1)

    store.search = '10.9.9.9'
    expect(mount(ConnectionTable).find('.empty').exists()).toBe(true)
  })

  it('備援路徑會標出來', () => {
    // 四條線一樣重的話，讀的人分不出平常的資料流是哪幾條。
    store.snapshot = fakeSnapshot([rows('c1'), rows('c2', { kind: 'fallback' })])
    const w = mount(ConnectionTable)
    expect(w.findAll('.fb')).toHaveLength(1)
    expect(w.findAll('tbody tr.fallback')).toHaveLength(1)
  })

  it('只看有問題會濾掉沒問題的列', () => {
    store.snapshot = fakeSnapshot([rows('c1'), rows('c2', { severity: 'error', rules: ['L004'] })])
    store.onlyProblems = true
    const w = mount(ConnectionTable)
    expect(w.findAll('tbody tr')).toHaveLength(1)
  })
})

describe('Lint 面板', () => {
  let store: ReturnType<typeof useProject>

  const findings: Finding[] = [
    {
      rule: 'L004', severity: 'error', environment: 'env-prod', subject: 'conn-1',
      end: 'to', detail: '期望 4 個，實際 3 個',
      fix: { count: { suggestion: 3 } },
    },
    {
      rule: 'L007', severity: 'warning', environment: 'env-dev', subject: 'conn-2',
      end: null, detail: '連線沒有填用途',
      fix: { text: { hint: '這條連線是做什麼用的', current: null } },
    },
  ]

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.snapshot = fakeSnapshot([rows('c1')], findings)
  })

  it('收合時只有一條摘要列', () => {
    const w = mount(LintPanel)
    expect(w.find('.list').exists()).toBe(false)
    expect(w.find('.bar').text()).toContain('1 錯誤')
    expect(w.find('.bar').text()).toContain('1 警告')
  })

  it('展開後列出每一項', () => {
    store.panelOpen = true
    const w = mount(LintPanel)
    expect(w.findAll('.list tbody tr')).toHaveLength(2)
  })

  it('點一項會跳到那個環境的連線分頁，並聚焦到那一項本身', () => {
    // 「知道有錯」到「看到那一列」之間不該需要自己找。
    store.panelOpen = true
    const w = mount(LintPanel)
    w.findAll('.list tbody .detail')[1]!.trigger('click')

    expect(store.view).toBe('資源')
    expect(store.resourceTab).toBe('連線')
    expect(store.selectedEnvironments).toEqual(['env-dev'])
    expect(store.search).toBe('')
    expect(store.focus?.subject).toBe('conn-2')
  })

  it('連點兩項不同的發現，第二次也要有反應', () => {
    // 這是真的踩過的：原本只切到「那個環境的有問題的列」，
    // 同一個環境裡連點兩項，畫面一模一樣，看起來就像第二次點壞掉。
    store.panelOpen = true
    const w = mount(LintPanel)

    w.findAll('.list tbody .detail')[0]!.trigger('click')
    const first = store.focus?.subject

    w.findAll('.list tbody .detail')[1]!.trigger('click')
    expect(store.focus?.subject).not.toBe(first)
  })

  it('聚焦時只留跟那一項有關的列', () => {
    store.snapshot = fakeSnapshot(
      [rows('c1'), rows('c2', { subjects: ['c2', 'r-cache'] })],
      findings,
    )
    store.focus = { subject: 'c2', label: 'x' }
    expect(store.visibleRows.map((r) => r.id)).toEqual(['c2'])
  })

  it('聚焦到沒有對應列的東西時，得到的是空清單而不是全部', () => {
    // 「篩不到就顯示全部」是最糟的：使用者以為那一項牽涉到每一條連線。
    store.snapshot = fakeSnapshot([rows('c1')], findings)
    store.focus = { subject: '邏輯層的東西', label: 'x' }
    expect(store.visibleRows).toEqual([])
  })

  it('修法的控制項完全由 Rust 送來的 fix 決定', async () => {
    // 前端不認得規則代號。同一個面板，L004 長出數字框、L007 長出文字框，
    // 差別只在 `fix` 的形狀——這裡若壞掉，代表有人在前端加了規則對照表。
    store.panelOpen = true
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
    store.panelOpen = true
    const applyFix = vi.spyOn(store, 'applyFix').mockResolvedValue(undefined)
    const w = mount(LintPanel)

    await w.findAll('.list .fix')[1]!.trigger('click')
    await w.find('.editor input').setValue('查快取')
    await w.find('.editor form').trigger('submit')

    expect(applyFix).toHaveBeenCalledWith(findings[1], { text: '查快取' })
  })

  it('數字那種修法送出的是數字', async () => {
    // 真的踩過：`<input type="number">` 的 v-model 會自動把值轉成數字，
    // 送出時對它呼叫 .trim() 直接炸掉，按下「套用」完全沒反應。
    // 只驗控制項長得對是不夠的——每一種修法都要真的送出一次。
    store.panelOpen = true
    const applyFix = vi.spyOn(store, 'applyFix').mockResolvedValue(undefined)
    const w = mount(LintPanel)

    await w.findAll('.list .fix')[0]!.trigger('click')
    await w.find('.editor input').setValue('4')
    await w.find('.editor form').trigger('submit')

    expect(applyFix).toHaveBeenCalledWith(findings[0], { count: 4 })
  })

  it('數字留空表示拿掉期望數量，不是不動它', async () => {
    store.panelOpen = true
    const applyFix = vi.spyOn(store, 'applyFix').mockResolvedValue(undefined)
    const w = mount(LintPanel)

    await w.findAll('.list .fix')[0]!.trigger('click')
    await w.find('.editor input').setValue('')
    await w.find('.editor form').trigger('submit')

    expect(applyFix).toHaveBeenCalledWith(findings[0], { count: null })
  })

  it('開關那種修法不需要輸入框', async () => {
    store.snapshot = fakeSnapshot([rows('c1')], [
      {
        rule: 'L008', severity: 'warning', environment: 'env-prod', subject: 'i-1',
        end: null, detail: '沒人碰', fix: { toggle: { label: '刻意獨立（冷備機等）' } },
      },
    ])
    store.panelOpen = true
    const applyFix = vi.spyOn(store, 'applyFix').mockResolvedValue(undefined)
    const w = mount(LintPanel)

    await w.find('.list .fix').trigger('click')
    expect(w.find('.editor input').exists()).toBe(false)
    expect(w.find('.editor').text()).toContain('刻意獨立')

    await w.find('.editor form').trigger('submit')
    expect(applyFix).toHaveBeenCalledWith(expect.objectContaining({ rule: 'L008' }), { toggle: true })
  })

  it('完全沒有修法的那幾條不放假按鈕', () => {
    // 按下去只會說「這個還沒做」的按鈕，比沒有按鈕更糟。
    store.snapshot = fakeSnapshot([rows('c1')], [
      { rule: 'L003', severity: 'error', environment: 'env-prod', subject: 'conn-9', end: null, detail: '指向不存在的機器', fix: null },
    ])
    store.panelOpen = true
    const w = mount(LintPanel)

    expect(w.find('.list .fix').exists()).toBe(false)
    expect(w.find('.list .act').text()).toContain('要改接')
  })

  it('L001／L002 走的是「補連線」表單，不是就地填一格', () => {
    // 這兩條是 lint 裡最重要的，之前畫面上只寫「要改連線」——
    // 工具指著問題叫，卻沒給任何辦法。
    store.snapshot = fakeSnapshot([rows('c1')], [
      {
        rule: 'L002', severity: 'error', environment: 'env-prod', subject: 'r-cache',
        end: null, detail: '從 api 走不到 redis',
        fix: { addConnection: { relationship: 'r-cache' } },
      },
    ])
    store.panelOpen = true
    const w = mount(LintPanel)

    expect(w.find('.list .fix').text()).toBe('補連線…')

    w.find('.list .fix').trigger('click')
    expect(store.addingConnection).toEqual({
      environment: 'env-prod',
      relationship: 'r-cache',
      label: '從 api 走不到 redis',
    })
  })

  it('補連線不會就地展開輸入框', () => {
    // 它要開的是一張表單，不是一格。兩個都跑出來就是兩套 UI 在打架。
    store.snapshot = fakeSnapshot([rows('c1')], [
      {
        rule: 'L001', severity: 'error', environment: 'env-prod', subject: 'r-cache',
        end: null, detail: '沒有任何實際連線',
        fix: { addConnection: { relationship: 'r-cache' } },
      },
    ])
    store.panelOpen = true
    const w = mount(LintPanel)

    w.find('.list .fix').trigger('click')
    expect(w.find('.editor').exists()).toBe(false)
  })

  it('沒問題時說一句話而不是空清單', () => {
    store.snapshot = fakeSnapshot([rows('c1')], [])
    store.panelOpen = true
    const w = mount(LintPanel)
    expect(w.find('.bar').text()).toContain('沒有發現問題')
    expect(w.find('.empty').text()).toContain('沒有任何缺漏')
  })
})

describe('連線表的排序', () => {
  let store: ReturnType<typeof useProject>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.snapshot = fakeSnapshot([
      rows('c1', { servesSlug: 'redis' }),
      rows('c2', { servesSlug: 'apache' }),
      rows('c3', { servesSlug: 'gateway' }),
    ])
  })

  /** 依表頭名稱取那一欄的全部值，不用位置——加一欄就全錯的測試沒有價值。 */
  function columnValues(w: ReturnType<typeof mount>, header: string) {
    const i = w.findAll('thead th').findIndex((t) => t.text() === header)
    return w.findAll('tbody tr').map((r) => r.findAll('td')[i]!.text())
  }

  const th = (w: ReturnType<typeof mount>, name: string) =>
    w.findAll('thead th').find((t) => t.text() === name)!

  it('點欄位標題會排序，再點一次倒過來', async () => {
    const w = mount(ConnectionTable)
    expect(columnValues(w, '契約')).toEqual(['redis', 'apache', 'gateway'])

    await th(w, '契約').trigger('click')
    expect(columnValues(w, '契約')).toEqual(['apache', 'gateway', 'redis'])

    await th(w, '契約').trigger('click')
    expect(columnValues(w, '契約')).toEqual(['redis', 'gateway', 'apache'])
  })

  it('第三次點回到原始順序', async () => {
    // Rust 給的順序是有意義的（依環境、再依契約）。排過就回不去的話那個資訊就沒了。
    const w = mount(ConnectionTable)
    await th(w, '契約').trigger('click')
    await th(w, '契約').trigger('click')
    await th(w, '契約').trigger('click')

    expect(columnValues(w, '契約')).toEqual(['redis', 'apache', 'gateway'])
    expect(th(w, '契約').attributes('aria-sort')).toBe('none')
  })

  it('排序狀態放在 aria-sort，不是只有一個箭頭', async () => {
    const w = mount(ConnectionTable)
    await th(w, '契約').trigger('click')
    expect(th(w, '契約').attributes('aria-sort')).toBe('ascending')
  })

  it('「實際／期望」照差幾台排，缺最多的在前面', async () => {
    // 點這一欄的人要找的就是對不上的那幾條，不是照數字大小看熱鬧。
    store.snapshot = fakeSnapshot([
      rows('c1', { servesSlug: '剛好', to: side('a', { matched: 3, expect: 3 }) }),
      rows('c2', { servesSlug: '缺三台', to: side('b', { matched: 1, expect: 4 }) }),
      rows('c3', { servesSlug: '缺一台', to: side('c', { matched: 2, expect: 3 }) }),
    ])
    const w = mount(ConnectionTable)
    await th(w, '實際／期望').trigger('click')
    expect(columnValues(w, '契約')).toEqual(['缺三台', '缺一台', '剛好'])
  })
})
