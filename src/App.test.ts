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
import type { Snapshot } from './lib/model'

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('./lib/bindings', () => ({
  commands: {
    openProject: vi.fn(),
    recheck: vi.fn(),
    saveProject: vi.fn(),
    previewImport: vi.fn(),
    applyImport: vi.fn(),
    cancelImport: vi.fn(),
  },
}))

function 假快照(): Snapshot {
  return {
    root: '/tmp/假的.loom',
    findings: [],
    rows: [],
    matrix: { relationships: [], environments: [], cells: [] },
    project: {
      id: 'p', slug: 'f', name: '假專案',
      logical: { people: [], systems: [], containers: [], relationships: [] },
      environments: [{ id: 'env-prod', slug: 'prod', name: '正式' }],
    },
  } as unknown as Snapshot
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
    store.snapshot = 假快照()
    const w = mount(App)
    expect(w.find('.welcome').exists()).toBe(false)
    expect(w.find('.toolbar').exists()).toBe(true)
  })

  it('匯入精靈不會影響歡迎畫面該不該出現', () => {
    // 精靈是疊在上層的對話框，跟「有沒有開專案」是兩件獨立的事。
    // 當初就是把它插進 v-if / v-else 中間才出事的。
    store.snapshot = 假快照()
    store.匯入中 = true
    const w = mount(App)
    expect(w.find('.welcome').exists()).toBe(false)
    expect(w.find('.backdrop').exists()).toBe(true)
  })

  it('切到連線表時矩陣要收起來', () => {
    store.snapshot = 假快照()
    store.檢視 = '連線表'
    const w = mount(App)
    expect(w.findComponent({ name: 'CoverageMatrix' }).exists()).toBe(false)
    expect(w.findComponent({ name: 'ConnectionTable' }).exists()).toBe(true)
  })

  it('沒開專案時不能按儲存與匯入', () => {
    const w = mount(App)
    const 停用的 = w.findAll('header button')
      .filter((b) => b.attributes('disabled') !== undefined)
      .map((b) => b.text())
    expect(停用的).toContain('儲存')
    expect(停用的).toContain('匯入試算表…')
  })
})
