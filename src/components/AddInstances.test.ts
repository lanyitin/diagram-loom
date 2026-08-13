/**
 * 批次建立機器的表單。
 *
 * 守的是：**按下去之前就看得到那幾行**，而且撞名要在那時候就講。
 * 一次建六台是會後悔的操作，建完再回頭數就太晚了。
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import AddInstances from './AddInstances.vue'
import { useProject } from '../lib/store'
import { commands } from '../lib/bindings'
import type { Snapshot } from '../lib/model'

vi.mock('../lib/bindings', () => ({ commands: { previewBatch: vi.fn() } }))

const threeNodes = {
  nodes: [{ id: 'n1' }, { id: 'n2' }, { id: 'n3' }],
  preview: [
    'vm-redis-01 / redis-01 @ 10.0.0.11:6379',
    'vm-redis-02 / redis-02 @ 10.0.0.12:6379',
    'vm-redis-03 / redis-03 @ 10.0.0.13:6379',
  ],
}

function fakeSnapshot(): Snapshot {
  return {
    root: '/x', findings: [], rows: [], dirty: false, undoLabel: null, redoLabel: null,
    matrix: { relationships: [], environments: [], cells: [] },
    project: {
      id: 'p', slug: 'f', name: 'n',
      logical: {
        people: [], systems: [], relationships: [],
        containers: [{
          id: 'c-redis', slug: 'redis', name: 'Redis', system: 's', external: false,
          endpoints: [{ id: 'e-redis', slug: 'client-port', protocol: 'tcp' }],
        }],
      },
      environments: [{
        id: 'env-dev', slug: 'dev', name: '開發',
        nodes: [{ id: 'n-site', slug: 'dc-main', kind: 'site', children: [] }],
      }],
    },
  } as unknown as Snapshot
}

describe('批次建立機器', () => {
  let store: ReturnType<typeof useProject>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.snapshot = fakeSnapshot()
    vi.mocked(commands.previewBatch).mockResolvedValue({ status: 'ok', data: threeNodes } as never)
  })

  async function openIt() {
    store.addingInstances = { environment: 'env-dev', container: 'c-redis', label: 'redis 一台都沒建' }
    const w = mount(AddInstances)
    await flushPromises()
    return w
  }

  it('沒有要建東西時整個不存在', () => {
    expect(mount(AddInstances).find('.box').exists()).toBe(false)
  })

  it('樣板依服務名稱預先填好', async () => {
    // 每次都從空白開始打 redis-{n}，六台建三次就會有人偷懶不補零。
    const w = await openIt()
    const values = w.findAll('input[type="text"]').map((i) => (i.element as HTMLInputElement).value)
    expect(values).toContain('redis-{n}')
    expect(values).toContain('vm-redis-{n}')
  })

  it('按下去之前就列出每一台', async () => {
    const w = await openIt()
    expect(w.findAll('.preview li')).toHaveLength(3)
    expect(w.find('.preview').text()).toContain('10.0.0.13:6379')
    expect(w.find('.primary').text()).toContain('3 台')
  })

  it('改了樣板就重算', async () => {
    const w = await openIt()
    vi.mocked(commands.previewBatch).mockClear()

    await w.findAll('input[type="number"]')[0]!.setValue(6)
    await flushPromises()

    expect(commands.previewBatch).toHaveBeenCalled()
    const spec = vi.mocked(commands.previewBatch).mock.calls[0]![1]
    expect(spec.count).toBe(6)
  })

  it('撞名時把 Rust 的話原樣顯示，而且不讓你建', async () => {
    vi.mocked(commands.previewBatch).mockResolvedValue({
      status: 'error', error: { message: 'redis-01 在這個環境已經有了' },
    } as never)
    const w = await openIt()

    expect(w.find('.blocked').text()).toContain('已經有了')
    expect(w.find('.preview').exists()).toBe(false)
    expect(w.find('.primary').attributes('disabled')).toBeDefined()
  })

  it('送出的是預覽算好的那批節點，不是重算一次', async () => {
    // 重算會產生新的 UUID，使用者拿到的就不是他剛剛看過的東西。
    const w = await openIt()
    const apply = vi.spyOn(store, 'applyEdit').mockResolvedValue(undefined)

    await w.find('.primary').trigger('click')
    expect(apply).toHaveBeenCalledWith({
      addInstances: { environment: 'env-dev', within: null, nodes: threeNodes.nodes },
    })
  })

  it('可以挑站點放進去', async () => {
    const w = await openIt()
    const apply = vi.spyOn(store, 'applyEdit').mockResolvedValue(undefined)

    const within = w.findAll('select').at(-1)!
    await within.setValue('n-site')
    await w.find('.primary').trigger('click')

    expect(apply).toHaveBeenCalledWith(
      expect.objectContaining({ addInstances: expect.objectContaining({ within: 'n-site' }) }),
    )
  })

  it('接點選單來自那個服務的定義', async () => {
    const w = await openIt()
    const options = w.findAll('select')[1]!.findAll('option').map((o) => o.text())
    expect(options).toEqual(['client-port'])
  })

  it('取消不會動到任何東西', async () => {
    const w = await openIt()
    const apply = vi.spyOn(store, 'applyEdit').mockResolvedValue(undefined)

    await w.findAll('footer button')[0]!.trigger('click')
    expect(store.addingInstances).toBeNull()
    expect(apply).not.toHaveBeenCalled()
  })
})
