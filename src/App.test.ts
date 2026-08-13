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
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import App from './App.vue'
import { useProject } from './lib/store'
import { commands } from './lib/bindings'
import type { Snapshot } from './lib/model'

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
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
    blankResource: vi.fn(),
    createProject: vi.fn(),
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

  it('切到資源檢視時另外兩個要收起來', () => {
    store.snapshot = fakeSnapshot()
    store.view = '資源'
    vi.mocked(commands.resourceTables).mockResolvedValue({ status: 'ok', data: [] } as never)
    const w = mount(App)
    expect(w.findComponent({ name: 'CoverageMatrix' }).exists()).toBe(false)
    expect(w.findComponent({ name: 'ConnectionTable' }).exists()).toBe(false)
    expect(w.findComponent({ name: 'ResourceView' }).exists()).toBe(true)
  })

  it('切到連線表時矩陣要收起來', () => {
    store.snapshot = fakeSnapshot()
    store.view = '連線表'
    const w = mount(App)
    expect(w.findComponent({ name: 'CoverageMatrix' }).exists()).toBe(false)
    expect(w.findComponent({ name: 'ConnectionTable' }).exists()).toBe(true)
    expect(w.findComponent({ name: 'ResourceView' }).exists()).toBe(false)
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
    store.view = '連線表'
    store.focus = { subject: 'conn-1', label: 'L006 redis-01／client-port 缺少位址' }
    const w = mount(App)

    expect(w.find('.focus').text()).toContain('缺少位址')
    await w.find('.focus').trigger('click')
    expect(store.focus).toBeNull()
    expect(w.find('.focus').exists()).toBe(false)
  })

  it('聚焦到沒有對應列的東西時講一句話，不是留一片空白', () => {
    store.snapshot = fakeSnapshot()
    store.view = '連線表'
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
