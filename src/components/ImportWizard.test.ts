/**
 * 匯入精靈的渲染測試。
 *
 * 這個流程的價值全在「使用者看得到將要發生什麼」，所以測的都是
 * **會不會有東西被藏起來**：警告有沒有顯示、前後值有沒有並排、
 * 沒有變更時套用鈕會不會誤導人。
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import ImportWizard from './ImportWizard.vue'
import type { Change, Plan } from '../lib/model'

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('../lib/bindings', () => ({
  commands: {
    previewImport: vi.fn(),
    applyImport: vi.fn(),
    cancelImport: vi.fn(),
  },
}))

function 變更(extra: Partial<Change> = {}): Change {
  return {
    kind: 'added',
    element: 'containerInstance',
    label: 'prod／redis-04',
    before: null,
    after: 'vm-redis-04',
    ...extra,
  }
}

function 計畫(extra: Partial<Plan> = {}): Plan {
  return { changes: [], unchanged: 0, warnings: [], environments: ['prod'], ...extra }
}

/** 直接把預覽結果塞進元件，跳過選檔那一步。 */
async function 帶著計畫掛載(p: Plan) {
  const w = mount(ImportWizard)
  ;(w.vm as unknown as { 計畫: Plan | null }).計畫 = p
  await w.vm.$nextTick()
  return w
}

describe('匯入精靈', () => {
  beforeEach(() => setActivePinia(createPinia()))

  it('一開始停在選檔，看不到套用鈕', () => {
    const w = mount(ImportWizard)
    expect(w.find('.pick').exists()).toBe(true)
    expect(w.text()).not.toContain('項變更')
  })

  it('預覽會分別數出新增與更新', async () => {
    const w = await 帶著計畫掛載(計畫({
      changes: [變更(), 變更({ label: 'prod／redis-05' }), 變更({ kind: 'updated', before: 'a', after: 'b' })],
      unchanged: 12,
    }))
    expect(w.find('.tally.add').text()).toBe('2 新增')
    expect(w.find('.tally.upd').text()).toBe('1 更新')
    expect(w.find('.tally.same').text()).toBe('12 沒有變化')
  })

  it('更新的項目把前後值並排', async () => {
    const w = await 帶著計畫掛載(計畫({
      changes: [變更({ kind: 'updated', element: 'address', label: 'prod／redis-01／client-port', before: '10.0.1.2:6379', after: '10.0.1.12:6379' })],
    }))
    expect(w.find('.was').text()).toBe('10.0.1.2:6379')
    expect(w.find('.now').text()).toBe('10.0.1.12:6379')
  })

  it('警告要顯示出來，不能靜靜吞掉', async () => {
    // 位址不一致時工具保留舊的。不講清楚，使用者會以為試算表上的新 IP
    // 已經寫進去了——這正是這個工具最不該犯的錯。
    const w = await 帶著計畫掛載(計畫({
      changes: [變更()],
      warnings: ['第 3 列：redis-01 已經是 10.0.1.11:6379，忽略不一致的 10.0.1.99:6379'],
    }))
    expect(w.find('.warnings').exists()).toBe(true)
    expect(w.find('.warnings').text()).toContain('10.0.1.99:6379')
  })

  it('會說清楚動到哪些環境', async () => {
    const w = await 帶著計畫掛載(計畫({ changes: [變更()], environments: ['prod', 'test'] }))
    expect(w.text()).toContain('prod、test')
  })

  it('沒有任何變更時說明白，而且不讓人按套用', async () => {
    const w = await 帶著計畫掛載(計畫({ unchanged: 47 }))
    expect(w.find('.nothing').text()).toContain('不會改變任何東西')
    expect(w.find('button.primary').attributes('disabled')).toBeDefined()
  })

  it('套用鈕上寫著會套用幾項', async () => {
    // 「確定」兩個字不會讓人停下來想；「套用 23 項變更」會。
    const w = await 帶著計畫掛載(計畫({ changes: [變更(), 變更({ label: 'x' })] }))
    expect(w.find('button.primary').text()).toBe('套用 2 項變更')
  })
})
