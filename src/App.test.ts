/**
 * 整個視窗的組裝測試。
 *
 * 這一層不管每個元件內部畫得對不對，只管**該出現的有出現、
 * 不該出現的沒出現**。
 *
 * 會有這份測試是因為真的踩過：在 `v-if` 與 `v-else` 中間插了一個
 * `<ImportWizard v-if>`，把 if/else 鏈打斷，結果歡迎畫面跟專案畫面
 * 同時渲染。型別檢查、單元測試、lint 全部都不會發現——
 * 只有真的把畫面組起來看才看得到。
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import App from './App.vue'
import { useProject } from './lib/store'
import { commands } from './lib/bindings'
import type { Snapshot } from './lib/model'

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
/** Rust 那邊 emit 的事件。測試裡自己觸發它。 */
const listeners: (() => void)[] = []
vi.mock('@tauri-apps/api/event', () => ({
  listen: (_name: string, handler: () => void) => {
    listeners.push(handler)
    return Promise.resolve(() => {})
  },
}))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    onCloseRequested: () => Promise.resolve(() => {}),
    destroy: vi.fn(),
  }),
}))
vi.mock('./lib/bindings', () => ({
  commands: {
    openProject: vi.fn(),
    recheck: vi.fn(),
    saveProject: vi.fn(),
    previewImport: vi.fn(),
    applyImport: vi.fn(),
    cancelImport: vi.fn(),
    previewEdit: vi.fn(),
    applyEdit: vi.fn(),
    applyFix: vi.fn(),
    undo: vi.fn(),
    redo: vi.fn(),
    proposeConnection: vi.fn(),
    connectionChoices: vi.fn(),
    previewBatch: vi.fn(),
    resourceTables: vi.fn(),
    diagramLinks: vi.fn(),
    diagramContracts: vi.fn(),
    blankResource: vi.fn(),
    createProject: vi.fn(),
    mcpStatus: vi.fn(),
    startMcp: vi.fn(),
    stopMcp: vi.fn(),
    mcpConfig: vi.fn(),
    setMcpConfig: vi.fn(),
    regenerateMcpToken: vi.fn(),
  },
}))

function fakeSnapshot(extra: Partial<Snapshot> = {}): Snapshot {
  return {
    root: '/tmp/假的.loom',
    findings: [],
    rows: [],
    matrix: { relationships: [], environments: [], cells: [] },
    dirty: false,
    undoLabel: null,
    redoLabel: null,
    project: {
      id: 'p', slug: 'f', name: '假專案',
      logical: { people: [], systems: [], containers: [], relationships: [] },
      environments: [{ id: 'env-prod', slug: 'prod', name: '正式' }],
    },
    ...extra,
  } as unknown as Snapshot
}

/** 依按鈕上的字找它。位置會變，字不會。 */
function headerButton(w: ReturnType<typeof mount>, text: string) {
  const b = w.findAll('header button').find((x) => x.text().includes(text))
  expect(b, `找不到「${text}」按鈕`).toBeTruthy()
  return b!
}

describe('視窗組裝', () => {
  let store: ReturnType<typeof useProject>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    // App 一掛載就會問一次端點狀態。沒有這個預設值，每個測試都會
    // 噴一個看不出來源的 unhandled rejection。
    vi.mocked(commands.mcpStatus).mockResolvedValue({
      status: 'ok',
      data: { running: false, url: null, preferredPort: null, requireToken: true, autostart: false },
    } as never)
  })

  it('沒開專案時只有歡迎畫面', () => {
    const w = mount(App)
    expect(w.find('.welcome').exists()).toBe(true)
    expect(w.find('.toolbar').exists()).toBe(false)
    expect(w.findComponent({ name: 'CoverageMatrix' }).exists()).toBe(false)
  })

  it('開了專案之後歡迎畫面要消失', () => {
    // 這正是踩過的那個 bug：兩個畫面疊在一起，下面那個把版面撐爛。
    store.snapshot = fakeSnapshot()
    const w = mount(App)
    expect(w.find('.welcome').exists()).toBe(false)
    expect(w.find('.toolbar').exists()).toBe(true)
  })

  it('匯入精靈不會影響歡迎畫面該不該出現', () => {
    // 精靈是疊在上層的對話框，跟「有沒有開專案」是兩件獨立的事。
    // 當初就是把它插進 v-if / v-else 中間才出事的。
    store.snapshot = fakeSnapshot()
    store.importing = true
    const w = mount(App)
    expect(w.find('.welcome').exists()).toBe(false)
    expect(w.find('.backdrop').exists()).toBe(true)
  })

  it('介面大小的三個級距在標頭上，不用開設定頁', () => {
    // 看不清楚的人第一件事就是找它。藏在兩層選單後面的無障礙設定等於沒有。
    const w = mount(App)
    const scale = w.find('.scale')
    expect(scale.exists()).toBe(true)
    expect(scale.findAll('button').map((b) => b.text())).toEqual(['小', '中', '大'])
    // 沒開專案也要能調——歡迎畫面的字一樣小。
    expect(scale.findAll('button').every((b) => b.attributes('disabled') === undefined)).toBe(true)
  })

  it('按了大就真的放大，而且記得住', async () => {
    const w = mount(App)
    await w.findAll('.scale button')[2]!.trigger('click')

    expect(document.documentElement.style.getPropertyValue('--zoom')).toBe('1.32')
    expect(localStorage.getItem('diagram-loom.ui-scale')).toBe('large')
  })

  it('歡迎畫面同時提供「從零開始」與「開啟現有」', () => {
    // 使用者不見得有 Excel 可以匯，從零開始是真實情境。
    const w = mount(App)
    const text = w.find('.welcome').text()
    expect(text).toContain('從零開始')
    expect(text).toContain('開啟現有專案')
  })

  /** 模式的兩顆鈕在標頭上，依字找。 */
  function modeButton(w: ReturnType<typeof mount>, text: string) {
    const b = w.findAll('header .mode button').find((x) => x.text() === text)
    expect(b, `找不到模式「${text}」`).toBeTruthy()
    return b!
  }

  it('模式切換在標頭上，跟復原／重做用一條線隔開', () => {
    // 兩組鈕擠在一起會被看成一組四顆。
    store.snapshot = fakeSnapshot()
    const w = mount(App)

    expect(w.findAll('header .mode button').map((b) => b.text())).toEqual(['總攬', '圖'])
    expect(w.find('header .divider').exists()).toBe(true)
  })

  it('切到圖時整條工具列不見，不是把搜尋藏起來', async () => {
    // 拆成兩層換掉的就是這塊補丁。搜尋與篩選是「列」的東西，
    // 而圖上沒有列——留一個打了字不會有反應的輸入框比拿掉更糟。
    store.snapshot = fakeSnapshot()
    const w = mount(App)
    expect(w.find('.toolbar').exists()).toBe(true)

    await modeButton(w, '圖').trigger('click')

    expect(store.mode).toBe('圖')
    expect(w.find('.toolbar').exists()).toBe(false)
    expect(w.findComponent({ name: 'DiagramView' }).exists()).toBe(true)
    expect(w.findComponent({ name: 'CoverageMatrix' }).exists()).toBe(false)
  })

  it('圖是模式不是檢視：去圖上晃一圈回來，還停在原本那一頁', async () => {
    // 這是拆開的實際好處。三個擠在同一排的時候，切到圖等於把
    // 「我剛剛在看資源」這件事丟掉，回來只能重點一次。
    store.snapshot = fakeSnapshot()
    store.view = '資源'
    vi.mocked(commands.resourceTables).mockResolvedValue({ status: 'ok', data: [] } as never)
    const w = mount(App)

    await modeButton(w, '圖').trigger('click')
    await modeButton(w, '總攬').trigger('click')

    expect(store.view).toBe('資源')
    expect(w.findComponent({ name: 'ResourceView' }).exists()).toBe(true)
  })

  it('沒開專案時不顯示模式切換', () => {
    // 沒有專案就沒有圖可以看，也沒有東西可以復原。
    expect(mount(App).find('header .mode').exists()).toBe(false)
  })

  it('切到資源檢視時另外兩個要收起來', () => {
    store.snapshot = fakeSnapshot()
    store.view = '資源'
    vi.mocked(commands.resourceTables).mockResolvedValue({ status: 'ok', data: [] } as never)
    const w = mount(App)
    expect(w.findComponent({ name: 'CoverageMatrix' }).exists()).toBe(false)
    expect(w.findComponent({ name: 'ConnectionTable' }).exists()).toBe(false)
    expect(w.findComponent({ name: 'ResourceView' }).exists()).toBe(true)
  })

  it('連線是資源底下的一個分頁，不是另一個檢視', () => {
    // 連線跟其他資源一樣是「這個專案裡有什麼」。分兩個地方放
    // 只是讓人多記一件事，所以它住在資源檢視裡面。
    store.snapshot = fakeSnapshot()
    store.view = '資源'
    store.resourceTab = '連線'
    const w = mount(App)
    expect(w.findComponent({ name: 'CoverageMatrix' }).exists()).toBe(false)
    expect(w.findComponent({ name: 'ResourceView' }).exists()).toBe(true)
    expect(w.findComponent({ name: 'ConnectionTable' }).exists()).toBe(true)
  })

  it('沒開專案時不能按儲存與匯入', () => {
    const w = mount(App)
    const disabled = w.findAll('header button')
      .filter((b) => b.attributes('disabled') !== undefined)
      .map((b) => b.text())
    expect(disabled).toContain('儲存')
    expect(disabled).toContain('匯入試算表…')
  })

  it('沒有未儲存的變更時儲存是停用的', () => {
    // 亮著的儲存鍵等於一直在說「你有事沒做」，看久了就沒意義了。
    store.snapshot = fakeSnapshot({ dirty: false })
    expect(headerButton(mount(App), '儲存').attributes('disabled')).toBeDefined()

    store.snapshot = fakeSnapshot({ dirty: true })
    const w = mount(App)
    expect(headerButton(w, '儲存').attributes('disabled')).toBeUndefined()
    expect(w.find('.dirty').exists()).toBe(true)
  })

  it('復原與重做各自看自己有沒有東西可做', () => {
    store.snapshot = fakeSnapshot({ undoLabel: '刪除連線', redoLabel: null })
    const w = mount(App)

    expect(headerButton(w, '復原').attributes('disabled')).toBeUndefined()
    expect(headerButton(w, '復原').attributes('title')).toBe('復原：刪除連線')
    expect(headerButton(w, '重做').attributes('disabled')).toBeDefined()
  })

  it('按了復原就往 Rust 送，畫面不自己算', async () => {
    store.snapshot = fakeSnapshot({ undoLabel: '修改用途' })
    const undo = vi.spyOn(store, 'undo').mockResolvedValue(undefined)

    await headerButton(mount(App), '復原').trigger('click')
    expect(undo).toHaveBeenCalled()
  })

  it('標頭看得出端點是開是關', async () => {
    // 使用者踩過一次：重開 App 之後端點預設是關的，而畫面上沒有任何地方
    // 講這件事——只能點進面板才知道。
    store.snapshot = fakeSnapshot()
    const w = mount(App)
    expect(w.find('.lamp').exists()).toBe(true)
    expect(w.find('.lamp.on').exists()).toBe(false)
    expect(headerButton(w, 'AI 助手').attributes('title')).toContain('關的')

    store.agentRunning = true
    await w.vm.$nextTick()
    expect(w.find('.lamp.on').exists()).toBe(true)
    expect(headerButton(w, 'AI 助手').attributes('title')).toContain('開著')
  })

  it('一開始就問一次端點的狀態', async () => {
    // 它可能是自動啟用的。不問的話那顆燈開機就是錯的。
    const refresh = vi.spyOn(store, 'refreshAgent').mockResolvedValue(undefined)
    mount(App)
    await flushPromises()
    expect(refresh).toHaveBeenCalled()
  })

  it('AI Agent 改了東西之後畫面會自己拉新', async () => {
    // 少了這個，Agent 做的事使用者完全看不到——而「看得到它在改什麼」
    // 正是把 MCP 掛在 App 裡而不是做成獨立程序的全部理由。
    listeners.length = 0
    store.snapshot = fakeSnapshot()
    const recheck = vi.spyOn(store, 'recheck').mockResolvedValue(undefined)
    mount(App)
    await flushPromises()

    expect(listeners.length, '沒有人在聽 loom://changed').toBe(1)
    listeners[0]!()
    expect(recheck).toHaveBeenCalled()
  })

  it('⌘Z 復原、⇧⌘Z 重做', async () => {
    // 只有一顆按鈕的復原，使用者不會相信它——他會改成「不敢亂按」。
    store.snapshot = fakeSnapshot({ undoLabel: '修改用途', redoLabel: '修改用途' })
    const undo = vi.spyOn(store, 'undo').mockResolvedValue(undefined)
    const redo = vi.spyOn(store, 'redo').mockResolvedValue(undefined)
    mount(App, { attachTo: document.body })

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', metaKey: true }))
    expect(undo).toHaveBeenCalled()
    expect(redo).not.toHaveBeenCalled()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', metaKey: true, shiftKey: true }))
    expect(redo).toHaveBeenCalled()
  })

  it('沒開專案時快捷鍵不做事', () => {
    const undo = vi.spyOn(store, 'undo').mockResolvedValue(undefined)
    mount(App, { attachTo: document.body })

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', metaKey: true }))
    expect(undo).not.toHaveBeenCalled()
  })

  it('聚焦時工具列會說出來，而且按一下就沒', async () => {
    // 看不見的篩選會讓使用者以為表格漏了東西。
    store.snapshot = fakeSnapshot()
    store.view = '資源'
    store.resourceTab = '連線'
    store.focus = { subject: 'conn-1', label: 'L006 redis-01／client-port 缺少位址' }
    const w = mount(App)

    expect(w.find('.focus').text()).toContain('缺少位址')
    await w.find('.focus').trigger('click')
    expect(store.focus).toBeNull()
    expect(w.find('.focus').exists()).toBe(false)
  })

  it('聚焦到沒有對應列的東西時講一句話，不是留一片空白', () => {
    store.snapshot = fakeSnapshot()
    store.view = '資源'
    store.resourceTab = '連線'
    store.focus = { subject: '邏輯層的東西', label: 'L007 …' }
    const w = mount(App)

    expect(w.find('.notice').exists()).toBe(true)
  })

  it('補連線的表單跟歡迎畫面互不影響', () => {
    store.snapshot = fakeSnapshot()
    vi.mocked(commands.proposeConnection).mockResolvedValue({
      status: 'ok',
      data: { id: 'c', serves: 'r-1', purpose: '', from: null, to: null, notes: [] },
    } as never)
    vi.mocked(commands.connectionChoices).mockResolvedValue({ status: 'ok', data: [] } as never)
    store.addingConnection = { environment: 'env-prod', relationship: 'r-1', label: 'x' }
    const w = mount(App)
    expect(w.find('.welcome').exists()).toBe(false)
    expect(w.findComponent({ name: 'AddConnection' }).exists()).toBe(true)
  })

  it('刪除確認框跟歡迎畫面互不影響', () => {
    // 跟匯入精靈同一個坑：它是疊在上層的對話框，不是 v-if 鏈的一環。
    store.snapshot = fakeSnapshot()
    vi.spyOn(store, 'previewEdit').mockResolvedValue({ introduced: [], resolved: [] })
    store.deleting = {
      edit: { deleteConnection: { environment: 'env-prod', connection: 'c1' } },
      kind: '連線', label: 'a → b',
    }
    const w = mount(App)
    expect(w.find('.welcome').exists()).toBe(false)
    expect(w.findComponent({ name: 'DeleteConfirm' }).find('.box').exists()).toBe(true)
  })
})
