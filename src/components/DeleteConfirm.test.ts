/**
 * 刪除確認框。
 *
 * 這裡守的是一件事：**框裡寫的必須是 Rust 算出來的影響，不是前端猜的。**
 * 所以測試餵進矛盾的資料（「會弄壞三件事」但清單裡寫別的），
 * 驗證畫面照著念，沒有自己重新判斷。
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import DeleteConfirm from './DeleteConfirm.vue'
import { useProject } from '../lib/store'
import type { Impact } from '../lib/model'

const 沒事: Impact = { introduced: [], resolved: [] }

const 會弄壞: Impact = {
  introduced: [
    { rule: 'L002', environment: 'env-prod', subject: 'r-cache', detail: '從 api 走不到 redis' },
    { rule: 'L008', environment: 'env-prod', subject: 'i-redis-01', detail: 'redis-01 沒有被任何連線碰到' },
  ],
  resolved: [],
}

describe('刪除確認框', () => {
  let store: ReturnType<typeof useProject>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
  })

  function 想刪() {
    store.刪除中 = {
      edit: { deleteConnection: { environment: 'env-prod', connection: 'conn-1' } },
      kind: '連線',
      label: 'api → redis',
    }
  }

  it('沒有要刪東西時整個不存在', () => {
    const w = mount(DeleteConfirm)
    expect(w.find('.box').exists()).toBe(false)
  })

  it('把 Rust 算出來的影響原樣列出來', async () => {
    vi.spyOn(store, '預覽編輯').mockResolvedValue(會弄壞)
    想刪()
    const w = mount(DeleteConfirm)
    await flushPromises()

    expect(w.findAll('.danger li')).toHaveLength(2)
    expect(w.find('.danger').text()).toContain('從 api 走不到 redis')
    expect(w.find('.danger').text()).toContain('L008')
  })

  it('不會弄壞東西時明講，而不是留一片空白', async () => {
    // 「你確定嗎」問三次就沒人看了。有影響要列出來，沒影響也要說。
    vi.spyOn(store, '預覽編輯').mockResolvedValue(沒事)
    想刪()
    const w = mount(DeleteConfirm)
    await flushPromises()

    expect(w.find('.danger').exists()).toBe(false)
    expect(w.find('.safe').text()).toContain('沒有因此多出任何問題')
  })

  it('問的是真的那條連線', async () => {
    const 預覽 = vi.spyOn(store, '預覽編輯').mockResolvedValue(沒事)
    想刪()
    mount(DeleteConfirm)
    await flushPromises()

    expect(預覽).toHaveBeenCalledWith({
      deleteConnection: { environment: 'env-prod', connection: 'conn-1' },
    })
  })

  it('取消不會動到任何東西', async () => {
    vi.spyOn(store, '預覽編輯').mockResolvedValue(會弄壞)
    const 套用 = vi.spyOn(store, '套用編輯').mockResolvedValue(undefined)
    想刪()
    const w = mount(DeleteConfirm)
    await flushPromises()

    await w.findAll('footer button')[0]!.trigger('click')
    expect(store.刪除中).toBeNull()
    expect(套用).not.toHaveBeenCalled()
  })

  it('確認後送出的是刪除那條連線', async () => {
    vi.spyOn(store, '預覽編輯').mockResolvedValue(會弄壞)
    const 套用 = vi.spyOn(store, '套用編輯').mockResolvedValue(undefined)
    想刪()
    const w = mount(DeleteConfirm)
    await flushPromises()

    await w.find('.danger-btn').trigger('click')
    expect(套用).toHaveBeenCalledWith({
      deleteConnection: { environment: 'env-prod', connection: 'conn-1' },
    })
    expect(store.刪除中).toBeNull()
  })

  it('告訴使用者刪錯了可以復原', async () => {
    // 這句話本身就是功能的一部分：不知道能復原的人不敢刪，
    // 於是舊連線永遠留著，表上的東西就不再等於真實環境。
    vi.spyOn(store, '預覽編輯').mockResolvedValue(沒事)
    想刪()
    const w = mount(DeleteConfirm)
    await flushPromises()

    expect(w.find('footer').text()).toContain('復原')
  })
})
