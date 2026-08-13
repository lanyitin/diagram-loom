/**
 * 關視窗前的攔截。
 *
 * 這是唯一會**永久弄丟使用者工作**的路徑，所以每一條分支都測：
 * 沒改東西不要煩人、改了要攔、存檔失敗絕對不能關。
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import CloseGuard from './CloseGuard.vue'
import { useProject } from '../lib/store'
import type { Snapshot } from '../lib/model'

const destroy = vi.fn()
/** Tauri 攔到關閉時會呼叫的那個 handler。測試裡自己觸發它。 */
let 攔截: ((e: { preventDefault: () => void }) => void) | null = null

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    onCloseRequested: (h: (e: { preventDefault: () => void }) => void) => {
      攔截 = h
      return Promise.resolve(() => {})
    },
    destroy,
  }),
}))

function 假快照(dirty: boolean): Snapshot {
  return {
    root: '/x', findings: [], rows: [], dirty, undoLabel: null, redoLabel: null,
    matrix: { relationships: [], environments: [], cells: [] },
    project: {
      id: 'p', slug: 'f', name: 'n',
      logical: { people: [], systems: [], containers: [], relationships: [] },
      environments: [],
    },
  } as unknown as Snapshot
}

describe('關視窗前的攔截', () => {
  let store: ReturnType<typeof useProject>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    destroy.mockClear()
    攔截 = null
  })

  /** 掛好元件，然後模擬使用者按下視窗的關閉鈕。 */
  async function 按下關閉(dirty: boolean) {
    store.snapshot = 假快照(dirty)
    const w = mount(CloseGuard)
    await flushPromises()

    const e = { preventDefault: vi.fn() }
    攔截!(e)
    await flushPromises()
    return { w, e }
  }

  it('沒有未儲存的變更時直接放行', async () => {
    // 每次關視窗都跳一個框，使用者三天後就會閉著眼睛按過去。
    const { w, e } = await 按下關閉(false)
    expect(e.preventDefault).not.toHaveBeenCalled()
    expect(w.find('.box').exists()).toBe(false)
  })

  it('有未儲存的變更時攔下來問', async () => {
    const { w, e } = await 按下關閉(true)
    expect(e.preventDefault).toHaveBeenCalled()
    expect(w.find('.box').exists()).toBe(true)
    expect(destroy).not.toHaveBeenCalled()
  })

  it('取消就留在原地，什麼都不動', async () => {
    const { w } = await 按下關閉(true)
    const 儲存 = vi.spyOn(store, '儲存')

    await w.findAll('footer button')[0]!.trigger('click')
    expect(w.find('.box').exists()).toBe(false)
    expect(destroy).not.toHaveBeenCalled()
    expect(儲存).not.toHaveBeenCalled()
  })

  it('選不存直接關就真的關掉', async () => {
    const { w } = await 按下關閉(true)
    const 儲存 = vi.spyOn(store, '儲存')

    await w.find('.danger-btn').trigger('click')
    expect(destroy).toHaveBeenCalled()
    expect(儲存).not.toHaveBeenCalled()
  })

  it('存檔並關閉會先存再關', async () => {
    const { w } = await 按下關閉(true)
    const 儲存 = vi.spyOn(store, '儲存').mockResolvedValue(undefined)

    await w.find('.primary').trigger('click')
    await flushPromises()

    expect(儲存).toHaveBeenCalled()
    expect(destroy).toHaveBeenCalled()
  })

  it('存檔失敗就不關——關掉的話錯誤訊息會跟著視窗一起消失', async () => {
    const { w } = await 按下關閉(true)
    vi.spyOn(store, '儲存').mockImplementation(async () => {
      store.錯誤 = '磁碟滿了'
    })

    await w.find('.primary').trigger('click')
    await flushPromises()

    expect(destroy).not.toHaveBeenCalled()
    expect(store.錯誤).toBe('磁碟滿了')
  })
})
