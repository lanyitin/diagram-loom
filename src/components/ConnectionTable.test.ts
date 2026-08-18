/**
 * 連線表與 Lint 面板的渲染測試。
 *
 * 一樣只驗「有沒有照實把 Rust 給的東西畫出來」。名稱解析與萬用字元展開
 * 由 `tests/connection_table.rs` 顧著。
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import ConnectionTable from './ConnectionTable.vue'
import LintPanel from './LintPanel.vue'
import { useProject } from '../lib/store'
import { commands } from '../lib/bindings'
import type { Finding, Row, Side, Snapshot } from '../lib/model'

vi.mock('../lib/bindings', () => ({ commands: { blankResource: vi.fn() } }))

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
    memo: '',
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

/** 欄位偏好會寫進 localStorage，同一個檔案的測試共用一份。 */
beforeEach(() => localStorage.clear())

describe('連線表', () => {
  let store: ReturnType<typeof useProject>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.snapshot = fakeSnapshot([rows('c1')])
  })

  it('挑欄位的選單裡有這張表全部的欄', () => {
    const w = mount(ConnectionTable)
    expect(w.findAll('.pop').length, '沒點就不該展開').toBe(0)

    w.find('thead .cols').trigger('click')
    return w.vm.$nextTick().then(() => {
      expect(w.findAll('.pop button').map((b) => b.text().replace('固定', '').trim()))
        .toEqual(['契約', '來源', '目標', '目標位址', '實際／期望', '用途', '備註'])
    })
  })

  it('關掉一欄之後那一欄整條不見，其他欄的值不會位移', async () => {
    // 欄位會依內容與使用者的選擇增減，所以一切都要用欄名認。
    const w = mount(ConnectionTable)
    await w.find('thead .cols').trigger('click')
    await w.findAll('.pop button')[3]!.trigger('click')   // 關掉「目標位址」

    expect(w.findAll('thead th[data-column]').map((t) => t.text()))
      .toEqual(['契約', '來源', '目標', '實際／期望', '用途', '備註'])
    expect(columnCell(w, '目標').text()).toBe('redis-* : client-port')
    expect(columnCell(w, '用途').text()).toBe('讀寫快取')
  })

  it('「實際／期望」整欄不出現時，也不會出現在選單裡', async () => {
    // 那不是使用者的偏好，是「這裡永遠是空白」。放進選單只會變成
    // 一個勾了也看不到東西的選項。
    store.snapshot = fakeSnapshot([rows('c1', { to: side('redis-01') })])
    const w = mount(ConnectionTable)
    await w.find('thead .cols').trigger('click')

    expect(w.findAll('.pop button').map((b) => b.text())).not.toContain('實際／期望')
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

  /**
   * 就地編輯。
   *
   * # 為什麼共用行為要用 `describe.each` 跑，不是每一欄抄一份
   *
   * 這一套機制的設計目的就是「服務任何一個可編欄位」。抄兩份的測試證明不了
   * 那件事——它只證明兩份都被寫對了一次。跑同一組才會在有人只修其中一欄時紅。
   */
  describe('就地編輯', () => {
    /** 依欄名點開那一格的編輯器。全域 `find` 在多欄可編之後會抓到別格。 */
    async function openEditor(w: ReturnType<typeof mount>, column: string) {
      await columnCell(w, column).find('button').trigger('click')
      return columnCell(w, column).find('input')
    }

    /** 這一欄現在存的值。 */
    const CURRENT: Record<string, Partial<Row>> = {
      用途: { purpose: '原本就有' },
      備註: { memo: '原本就有' },
    }

    describe.each(['用途', '備註'])('%s', (column) => {
      it('沒填過的那一列也點得開', async () => {
        // 只在有值時才顯示的話，空的那幾條永遠填不了第一次——
        // 而最需要填的正是空的那幾條（L007 叫的就是空用途）。
        store.snapshot = fakeSnapshot([rows('c1', { purpose: '', memo: '' })])
        const w = mount(ConnectionTable)
        expect(columnCell(w, column).find('button').exists()).toBe(true)

        const box = await openEditor(w, column)
        expect(box.exists()).toBe(true)
      })

      it('沒改就不留下一步復原', async () => {
        // 點開又關掉多一步 ⌘Z，而那一步什麼都沒做——使用者按下去會以為
        // 自己退掉了真的東西。
        store.snapshot = fakeSnapshot([rows('c1', CURRENT[column]!)])
        const w = mount(ConnectionTable)
        const applyEdit = vi.spyOn(store, 'applyEdit').mockResolvedValue()

        const box = await openEditor(w, column)
        await box.trigger('blur')

        expect(applyEdit).not.toHaveBeenCalled()
      })

      it('按 Esc 就整個不算', async () => {
        store.snapshot = fakeSnapshot([rows('c1', CURRENT[column]!)])
        const w = mount(ConnectionTable)
        const applyEdit = vi.spyOn(store, 'applyEdit').mockResolvedValue()

        const box = await openEditor(w, column)
        await box.setValue('改到一半反悔')
        await box.trigger('keydown.esc')

        expect(applyEdit).not.toHaveBeenCalled()
        expect(columnCell(w, column).text()).toBe('原本就有')
      })

      it('送出前削掉前後空白', async () => {
        // Rust 只有 `SetPurpose` 會削，`SetConnectionMemo` 是原樣存的。
        // 前端不削的話，「還沒決定　」跟「還沒決定」是兩份不同的資料，
        // 而畫面上長得一模一樣。
        store.snapshot = fakeSnapshot([rows('c1', { purpose: '', memo: '' })])
        const w = mount(ConnectionTable)
        const applyEdit = vi.spyOn(store, 'applyEdit').mockResolvedValue()

        const box = await openEditor(w, column)
        await box.setValue('  還沒決定  ')
        await box.trigger('blur')

        const sent = JSON.stringify(applyEdit.mock.calls[0]?.[0])
        expect(sent).toContain('還沒決定')
        expect(sent).not.toContain(' 還沒決定')
      })

      it('編這一格的時候，同一列的另一格不會跟著變成輸入框', async () => {
        // 只認「哪一列」的話兩格會一起打開、綁同一個 draft，於是 blur 哪一個
        // 都照那一格的 Edit 送出——**用途的字會被寫進備註**。沒有錯誤訊息，
        // 而備註沒有規則在看，永遠不會被發現。
        const other = column === '用途' ? '備註' : '用途'
        const w = mount(ConnectionTable)
        await openEditor(w, column)

        expect(columnCell(w, other).find('input').exists()).toBe(false)
      })
    })

    it('改用途送出的是 setPurpose，而且帶著環境', async () => {
      // `environment` 給 null 的話 Rust 會去邏輯層找同 id 的**契約**，
      // 而連線 id 在那裡不存在 → 整次失敗，畫面上只有「按了沒反應」。
      const w = mount(ConnectionTable)
      const applyEdit = vi.spyOn(store, 'applyEdit').mockResolvedValue()

      const box = await openEditor(w, '用途')
      await box.setValue('讀寫工作階段快取')
      await box.trigger('blur')

      expect(applyEdit).toHaveBeenCalledWith({
        setPurpose: { environment: 'env-prod', subject: 'c1', purpose: '讀寫工作階段快取' },
      })
    })

    it('改備註送出的是 setConnectionMemo，不是改用途', async () => {
      // 兩個欄位是刻意分開的：L007 在看 `purpose`。寫進同一格的話，
      // 「等年底汰換」會讓 L007 從此不再叫。
      const w = mount(ConnectionTable)
      const applyEdit = vi.spyOn(store, 'applyEdit').mockResolvedValue()

      const box = await openEditor(w, '備註')
      await box.setValue('等年底汰換')
      await box.trigger('blur')

      expect(applyEdit).toHaveBeenCalledWith({
        setConnectionMemo: { environment: 'env-prod', connection: 'c1', memo: '等年底汰換' },
      })
    })

    it('用途清空是合法的', async () => {
      // 清空讓 L007 重新叫，那正是「我還不知道」該有的狀態。
      const w = mount(ConnectionTable)
      const applyEdit = vi.spyOn(store, 'applyEdit').mockResolvedValue()

      const box = await openEditor(w, '用途')
      await box.setValue('')
      await box.trigger('blur')

      expect(applyEdit).toHaveBeenCalledWith({
        setPurpose: { environment: 'env-prod', subject: 'c1', purpose: '' },
      })
    })

    it('備註清空是合法的', async () => {
      // 備註沒有規則在看，寫錯了沒有第二條路可以救。
      store.snapshot = fakeSnapshot([rows('c1', { memo: '寫錯了' })])
      const w = mount(ConnectionTable)
      const applyEdit = vi.spyOn(store, 'applyEdit').mockResolvedValue()

      const box = await openEditor(w, '備註')
      await box.setValue('')
      await box.trigger('blur')

      expect(applyEdit).toHaveBeenCalledWith({
        setConnectionMemo: { environment: 'env-prod', connection: 'c1', memo: '' },
      })
    })
  })

  /**
   * 正常 ⇄ 備援。
   *
   * 這件事以前**只有 Agent（MCP）做得到**：`kind` 只在新增連線時勾得到，
   * 建完就再也改不了。人做不到而 Agent 做得到，是最糟的分工。
   */
  describe('切換正常／備援', () => {
    const toggle = (w: ReturnType<typeof mount>) => w.find('.kindtoggle')

    it('切成備援送出的是 setConnectionKind', async () => {
      const w = mount(ConnectionTable)
      const applyEdit = vi.spyOn(store, 'applyEdit').mockResolvedValue()

      await toggle(w).trigger('click')

      expect(applyEdit).toHaveBeenCalledWith({
        setConnectionKind: { environment: 'env-prod', connection: 'c1', kind: 'fallback' },
      })
    })

    it('再切一次切得回正常', async () => {
      // 防的是寫死 `kind: 'fallback'` 的單向切換——症狀是「切錯了退不回來，
      // 只能刪掉重建」，而重建會換一個新的 id，圖上綁著它的標註會斷。
      store.snapshot = fakeSnapshot([rows('c1', { kind: 'fallback' })])
      const w = mount(ConnectionTable)
      const applyEdit = vi.spyOn(store, 'applyEdit').mockResolvedValue()

      await toggle(w).trigger('click')

      expect(applyEdit).toHaveBeenCalledWith({
        setConnectionKind: { environment: 'env-prod', connection: 'c1', kind: 'primary' },
      })
    })

    it('正常的那一列也點得到', async () => {
      // 只在 fallback 才給按鈕的話，就沒有任何路徑把一條連線標成備援——
      // 那正是這次要補的洞。
      const w = mount(ConnectionTable)
      expect(toggle(w).exists()).toBe(true)
    })

    it('切換鈕說得出現在是哪一種', () => {
      // 「正常」那一格平常是留白的，看不見就不能只靠看。
      const w = mount(ConnectionTable)
      expect(toggle(w).attributes('aria-pressed')).toBe('false')
      expect(toggle(w).attributes('title')).toContain('正常路徑')

      store.snapshot = fakeSnapshot([rows('c1', { kind: 'fallback' })])
      const f = mount(ConnectionTable)
      expect(toggle(f).attributes('aria-pressed')).toBe('true')
      expect(toggle(f).attributes('title')).toContain('備援路徑')
    })
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

  it('只能手動處理的那幾條不放假按鈕，而且那句話是 Rust 給的', () => {
    // 按下去只會說「這個還沒做」的按鈕，比沒有按鈕更糟。
    //
    // 這句提示原本寫死成「要改接」——那是規則知識放在畫面上，而且只對
    // L003 與 L012 成立：L013 要對調兩端、L014 要改名字。現在由
    // `Fix::Manual` 帶過來，前端照著印就好。
    store.snapshot = fakeSnapshot([rows('c1')], [
      {
        rule: 'L014', severity: 'warning', environment: 'env-prod', subject: 'i-1',
        end: null, detail: '兩個東西同名',
        fix: { manual: { hint: '改掉其中一個的名字' } },
      } as unknown as Finding,
    ])
    store.panelOpen = true
    const w = mount(LintPanel)

    expect(w.find('.list .fix').exists(), '不該有按鈕').toBe(false)
    expect(w.find('.list .act').text()).toBe('改掉其中一個的名字')
    expect(w.find('.editor').exists(), '也不該就地展開輸入框').toBe(false)
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

describe('外部系統沒有實體時的修法', () => {
  let store: ReturnType<typeof useProject>

  /** L001 報在**邏輯層那個外部系統**上：它在這個環境還沒有實體。 */
  const noInstance: Finding = {
    rule: 'L001', severity: 'error', environment: 'env-prod', subject: 's-sso',
    end: null, detail: '外部系統 sso 在環境 prod 沒有指定位址',
    fix: { addResource: { kind: 'systemInstance', environment: 'env-prod', owner: 's-sso' } },
  } as unknown as Finding

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.snapshot = fakeSnapshot([rows('c1')], [noInstance])
    store.panelOpen = true
    // 兩個測試都會按到那顆鈕，沒有預設值的話會噴一個看不出來源的 rejection。
    vi.mocked(commands.blankResource).mockResolvedValue({
      status: 'ok',
      data: { systemInstance: { environment: 'env-prod', instance: { id: 'si-新的' } } },
    } as never)
  })

  it('給的是「開一張表單」的鈕，不是就地填一格', async () => {
    // 它要一個名字跟一個位址，兩個都只有人知道。填一格的框裝不下。
    const w = mount(LintPanel)
    expect(w.find('.list .fix').text()).toBe('建實體…')

    await w.find('.list .fix').trigger('click')
    expect(w.find('.editor').exists(), '不該同時就地展開').toBe(false)
  })

  it('空白的那份跟 Rust 要，不是前端自己拼', async () => {
    // id 要在 Rust 發好，apply 才是決定性的——復原之後重做要拿到同一個
    // 元素，不是一個新 UUID。
    const w = mount(LintPanel)

    await w.find('.list .fix').trigger('click')
    await flushPromises()

    expect(commands.blankResource).toHaveBeenCalledWith('systemInstance', 'env-prod', 's-sso')
    expect(store.editingResource?.isNew).toBe(true)
    expect(store.editingResource?.kind).toBe('外部系統實體')
  })
})
