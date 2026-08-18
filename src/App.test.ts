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
/**
 * Rust 那邊 emit 的事件。測試裡自己觸發它。
 *
 * 分事件名記錄：現在有兩個（`loom://changed` 與 `loom://menu`），
 * 混在一個陣列裡的話，測試會變成靠註冊順序猜——加第三個就會壞。
 */
const listeners: Record<string, ((e: { payload: unknown }) => void)[]> = {}
vi.mock('@tauri-apps/api/event', () => ({
  listen: (name: string, handler: (e: { payload: unknown }) => void) => {
    ;(listeners[name] ??= []).push(handler)
    return Promise.resolve(() => {})
  },
}))

/** 按下系統選單裡的某一項。 */
async function menu(id: string) {
  const handlers = listeners['loom://menu'] ?? []
  expect(handlers.length, '沒有人在聽 loom://menu').toBe(1)
  handlers[0]!({ payload: id })
  await flushPromises()
}
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
    diagramLinks: vi.fn().mockResolvedValue({ status: 'ok', data: [] }),
    diagramContracts: vi.fn(),
    // 畫布現在是真的會建起來的（以前那個 iframe 在測試裡永遠不會 ready），
    // 所以「圖」那一頁一掛載就會去問這幾個。
    // 給預設值：回 undefined 的話，元件讀 `res.status` 會炸在 Promise 裡，
    // 而那種錯誤不會讓測試變紅，只會變成一行「unhandled rejection」。
    diagramCatalog: vi.fn().mockResolvedValue({
      status: 'ok',
      data: {
        diagrams: [],
        model: '',
        deployment: 'diagram-loom-deployment',
        context: 'diagram-loom-context',
      },
    }),
    diagramRead: vi.fn(),
    diagramSave: vi.fn(),
    diagramCreate: vi.fn(),
    diagramFocus: vi.fn().mockResolvedValue({
      status: 'ok',
      data: { contracts: [], highlight: { shapes: [], connections: [] }, shapes: 0 },
    }),
    diagramTargets: vi.fn().mockResolvedValue({
      status: 'ok',
      data: { shapes: [], connections: [], guesses: [] },
    }),
    blankResource: vi.fn(),
    createProject: vi.fn(),
    newWindow: vi.fn(),
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
    // 每個測試各自掛一次 App，不清掉的話監聽會累積，
    // 而「有幾個人在聽」正是底下幾條測試在斷言的東西。
    for (const key of Object.keys(listeners)) delete listeners[key]
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

  it('「新視窗」隨時都叫得動，就算還沒開專案', async () => {
    // 要同時看兩個專案就是從這裡開始。空視窗上也要能用——
    // 不然使用者得先開一個專案才生得出第二扇窗。
    vi.mocked(commands.newWindow).mockResolvedValue({ status: 'ok', data: null } as never)
    mount(App)
    await flushPromises()

    await menu('new-window')
    expect(commands.newWindow).toHaveBeenCalled()
  })

  it('有未儲存時「開啟專案…」會先問，不直接蓋掉', async () => {
    // 開啟專案是**取代**這扇視窗看的東西。多視窗之後它變成日常操作，
    // 沒問就蓋掉等於安靜地丟掉工作。
    const { open } = await import('@tauri-apps/plugin-dialog')
    vi.mocked(open).mockResolvedValue('/tmp/另一個.loom' as never)
    store.snapshot = fakeSnapshot({ dirty: true })
    const openProject = vi.spyOn(store, 'open').mockResolvedValue(undefined)
    const w = mount(App)
    await flushPromises()

    await menu('open-project')

    expect(openProject, '還沒問就開了').not.toHaveBeenCalled()
    const ask = w.find('[aria-label="有未儲存的變更"]')
    expect(ask.exists()).toBe(true)
    // 三個選項都要在。少了「存了再開」的話，使用者得先取消、自己按儲存、
    // 再開一次——三步做一件事。
    expect(ask.text()).toContain('存了再開')
    expect(ask.text()).toContain('不存直接開')
    expect(ask.text()).toContain('取消')
  })

  it('沒有未儲存時就直接開，不要多問一句', async () => {
    const { open } = await import('@tauri-apps/plugin-dialog')
    vi.mocked(open).mockResolvedValue('/tmp/另一個.loom' as never)
    store.snapshot = fakeSnapshot({ dirty: false })
    const openProject = vi.spyOn(store, 'open').mockResolvedValue(undefined)
    const w = mount(App)
    await flushPromises()

    await menu('open-project')

    expect(openProject).toHaveBeenCalledWith('/tmp/另一個.loom')
    expect(w.find('[aria-label="有未儲存的變更"]').exists()).toBe(false)
  })

  it('那個專案已經開在別的視窗時，這扇窗維持原狀', async () => {
    // Rust 會把那扇窗叫到前面。這扇窗**不能**跟著換過去——
    // 換過去的話同一個專案就有兩份 History，兩邊各自存檔會互相蓋掉。
    store.snapshot = fakeSnapshot()
    const before = store.snapshot
    vi.mocked(commands.openProject).mockResolvedValue({
      status: 'ok',
      data: { elsewhere: { name: 'payments' } },
    } as never)

    await store.open('/tmp/payments.loom')

    expect(store.snapshot, '這扇視窗被換掉了').toBe(before)
    expect(store.notice).toContain('payments')
    expect(store.error).toBeNull()
  })

  it('「已經開在別的視窗」用一句話講，不是紅字', () => {
    // 使用者沒做錯什麼，他只是選了一個他已經在看的東西。
    store.snapshot = fakeSnapshot()
    store.notice = '「payments」已經開在另一個視窗了，我把它叫到前面。'
    const w = mount(App)

    expect(w.find('.notice-bar').text()).toContain('已經開在另一個視窗')
    expect(w.find('.failure').exists(), '不該用錯誤的紅字').toBe(false)
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

  it('沒開專案時不能按儲存', () => {
    const w = mount(App)
    const disabled = w.findAll('header button')
      .filter((b) => b.attributes('disabled') !== undefined)
      .map((b) => b.text())
    expect(disabled).toContain('儲存')
  })

  it('還沒開專案時，畫面上要有一條看得見的路', () => {
    // 選單列不是每個人第一件事就會去看的地方，而一扇空視窗沒有別的線索。
    const w = mount(App)
    const labels = w.findAll('header button').map((b) => b.text())
    expect(labels).toContain('開啟專案…')
    expect(labels).toContain('新專案…')
  })

  it('開了專案之後那兩顆就收起來，日常操作走選單', () => {
    // 每天在看的是表格，不是標頭。做完就離開的動作不該一直佔位置。
    store.snapshot = fakeSnapshot()
    const labels = mount(App).findAll('header button').map((b) => b.text())
    for (const gone of ['開啟專案…', '新專案…', '新視窗', '匯入試算表…', '復原', '重做']) {
      expect(labels.some((t) => t.includes(gone)), `${gone} 應該只在選單裡`).toBe(false)
    }
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

  it('按了復原就往 Rust 送，畫面不自己算', async () => {
    // 「有沒有東西可以復原」現在由選單項目的灰不灰表示，而那是 Rust
    // 照 `History::undo_label` 設的（見 `menu.rs`）。前端只負責轉送。
    store.snapshot = fakeSnapshot({ undoLabel: '修改用途' })
    const undo = vi.spyOn(store, 'undo').mockResolvedValue(undefined)
    mount(App)
    await flushPromises()

    await menu('undo')
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
    store.snapshot = fakeSnapshot()
    const recheck = vi.spyOn(store, 'recheck').mockResolvedValue(undefined)
    mount(App)
    await flushPromises()

    const changed = listeners['loom://changed'] ?? []
    expect(changed.length, '沒有人在聽 loom://changed').toBe(1)
    changed[0]!({ payload: null })
    expect(recheck).toHaveBeenCalled()
  })

  it('快捷鍵不再自己聽 keydown', async () => {
    // ⚠️ 這條當初守的是一個安靜的 bug：draw.io 跑在跨 origin 的 iframe 裡，
    // keydown 不會冒泡給父文件，所以掛在 window 上的 ⌘S 完全沒有存到東西。
    //
    // **那個 iframe 已經不在了**，但這條要留著，理由換成：畫布自己在
    // document 上聽鍵盤，這裡再聽一份就是兩個人搶同一個組合鍵。
    store.snapshot = fakeSnapshot({ undoLabel: '修改用途' })
    const undo = vi.spyOn(store, 'undo').mockResolvedValue(undefined)
    const save = vi.spyOn(store, 'save').mockResolvedValue(undefined)
    mount(App, { attachTo: document.body })
    await flushPromises()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', metaKey: true }))
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 's', metaKey: true }))
    expect(undo).not.toHaveBeenCalled()
    expect(save).not.toHaveBeenCalled()

    // 同一件事改走選單就會動。
    await menu('undo')
    expect(undo).toHaveBeenCalled()
  })

  it('選單每一項都有人接', async () => {
    // Rust 那邊列了哪些 id 是一份契約。前端漏接一項的話，按下去
    // **什麼都不會發生，而且沒有錯誤**——所以照著 menu.rs 對一次。
    const { readFileSync } = await import('node:fs')
    const { join } = await import('node:path')
    const rust = readFileSync(
      join(import.meta.dirname, '..', 'src-tauri', 'src', 'menu.rs'), 'utf8',
    )
    const declared = rust
      .split('pub const ACTIONS')[1]!.split('];')[0]!
      .match(/"([a-z-]+)"/g)!.map((q) => q.slice(1, -1))
    const handled = readFileSync(join(import.meta.dirname, 'App.vue'), 'utf8')
      .split('const MENU')[1]!.split('\n}')[0]!

    expect(declared.length).toBeGreaterThan(0)
    for (const id of declared) {
      expect(handled.includes(`'${id}'`) || handled.includes(`\n  ${id}:`), `沒人接 ${id}`).toBe(true)
    }
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
