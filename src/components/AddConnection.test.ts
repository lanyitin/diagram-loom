/**
 * 補一條連線的表單。
 *
 * 守的是兩件事：
 * ① 提案的假設**一定要顯示出來**——擬得像真的卻不說，使用者會直接按下去。
 * ② 選單裡的 `endpointing` 是**不透明值**，原樣送回去。
 *    這裡若出現任何加工，就是模型知識漏到前端了。
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import AddConnection from './AddConnection.vue'
import { useProject } from '../lib/store'
import { commands } from '../lib/bindings'
import type { Choice, Proposal, Snapshot } from '../lib/model'

vi.mock('../lib/bindings', () => ({
  commands: {
    proposeConnection: vi.fn(),
    connectionChoices: vi.fn(),
  },
}))

const API = { one: 'i-api-01' }
const REDIS = { pattern: { slugPattern: 'redis-*', expect: 3 } }

const fromChoices: Choice[] = [
  { kind: 'instance', label: 'api-01（不指定接點）', group: '服務', endpointing: { instance: { target: API } } },
  { kind: 'person', label: 'customer', group: '人', endpointing: { person: { person: 'p-1' } } },
] as unknown as Choice[]

const target: Choice[] = [
  { kind: 'instance', label: 'redis-* : client-port（3 台）', group: '服務（整群）', endpointing: { instance: { target: REDIS, endpoint: 'e-redis' } } },
  { kind: 'infra', label: 'f5-01 : vip', group: '設備', endpointing: { infra: { node: 'n-f5', endpoint: 'ep-vip' } } },
] as unknown as Choice[]

function proposal(extra: Partial<Proposal> = {}): Proposal {
  return {
    id: 'conn-新的',
    serves: 'r-cache',
    purpose: '訂單服務讀寫快取',
    from: fromChoices[0]!.endpointing,
    to: target[0]!.endpointing,
    notes: ['擬的是直達的一段。若這條流量其實會經過 F5 之類的設備，請改成兩段。'],
    ...extra,
  }
}

function fakeSnapshot(): Snapshot {
  return {
    root: '/x', findings: [], rows: [], dirty: false, undoLabel: null, redoLabel: null,
    matrix: { relationships: [], environments: [], cells: [] },
    project: {
      id: 'p', slug: 'f', name: 'n',
      logical: { people: [], systems: [], containers: [], relationships: [] },
      environments: [{ id: 'env-prod', slug: 'prod', name: '正式' }],
    },
  } as unknown as Snapshot
}

describe('補一條連線', () => {
  let store: ReturnType<typeof useProject>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.snapshot = fakeSnapshot()
    vi.mocked(commands.connectionChoices).mockImplementation(async (_env, end) =>
      ({ status: 'ok', data: end === 'from' ? fromChoices : target }) as never,
    )
  })

  async function openIt(p: Proposal = proposal()) {
    vi.mocked(commands.proposeConnection).mockResolvedValue({ status: 'ok', data: p } as never)
    store.addingConnection = { environment: 'env-prod', relationship: 'r-cache', label: '沒有任何實際連線' }
    const w = mount(AddConnection)
    await flushPromises()
    return w
  }

  it('沒有要補東西時整個不存在', () => {
    expect(mount(AddConnection).find('.box').exists()).toBe(false)
  })

  it('把提案的假設列出來', async () => {
    // 工具不知道中間要不要經過 F5。擬得像真的卻不說，使用者會直接按下去。
    const w = await openIt()
    expect(w.findAll('.notes li')).toHaveLength(1)
    expect(w.find('.notes').text()).toContain('直達')
  })

  it('用途預先帶進契約的用途，不留白', async () => {
    const w = await openIt()
    expect((w.find('input[type="text"]').element as HTMLInputElement).value)
      .toBe('訂單服務讀寫快取')
  })

  it('兩個選單都照 Rust 給的分組列出來', async () => {
    const w = await openIt()
    const groups = w.findAll('optgroup').map((g) => g.attributes('label'))
    expect(groups).toContain('服務（整群）')
    expect(groups).toContain('設備')
    expect(groups).toContain('人')
  })

  it('送出的 endpointing 是選單原樣給的，前端不加工', async () => {
    const w = await openIt()
    const apply = vi.spyOn(store, 'applyEdit').mockResolvedValue(undefined)

    await w.find('.primary').trigger('click')

    expect(apply).toHaveBeenCalledWith({
      addConnection: {
        environment: 'env-prod',
        id: 'conn-新的',
        serves: 'r-cache',
        purpose: '訂單服務讀寫快取',
        kind: 'primary',
        from: fromChoices[0]!.endpointing,
        to: target[0]!.endpointing,
      },
    })
  })

  it('換掉某一端之後送的是換過的那個', async () => {
    const w = await openIt()
    const apply = vi.spyOn(store, 'applyEdit').mockResolvedValue(undefined)

    // 改成走 F5（提案擬不出這件事，只有人知道）。
    const toSelect = w.findAll('select')[1]!
    await toSelect.setValue('1')
    await w.find('.primary').trigger('click')

    expect(apply).toHaveBeenCalledWith(
      expect.objectContaining({
        addConnection: expect.objectContaining({ to: target[1]!.endpointing }),
      }),
    )
  })

  it('勾了備援就送 fallback', async () => {
    const w = await openIt()
    const apply = vi.spyOn(store, 'applyEdit').mockResolvedValue(undefined)

    await w.find('input[type="checkbox"]').setValue(true)
    await w.find('.primary').trigger('click')

    expect(apply).toHaveBeenCalledWith(
      expect.objectContaining({ addConnection: expect.objectContaining({ kind: 'fallback' }) }),
    )
  })

  it('有一端擬不出來時不能建立，而且說得出為什麼', async () => {
    // 服務一台都還沒建的時候，「請選擇來源」是個無解的問題。
    const w = await openIt(proposal({ to: null, notes: ['目標的服務 redis 在 prod 一台都還沒建，要先建機器。'] }))

    expect(w.find('.primary').attributes('disabled')).toBeDefined()
    expect(w.find('.notes').text()).toContain('一台都還沒建')
  })

  it('取消不會動到任何東西', async () => {
    const w = await openIt()
    const apply = vi.spyOn(store, 'applyEdit').mockResolvedValue(undefined)

    await w.findAll('footer button')[0]!.trigger('click')
    expect(store.addingConnection).toBeNull()
    expect(apply).not.toHaveBeenCalled()
  })

  it('用的是提案發好的 id，不是自己產一個', async () => {
    // id 在提案時就發好，套用才是決定性的：預覽算出來的就是套用後的樣子，
    // 復原重做也會重播出同一條連線。
    const w = await openIt()
    const apply = vi.spyOn(store, 'applyEdit').mockResolvedValue(undefined)

    await w.find('.primary').trigger('click')
    expect(apply).toHaveBeenCalledWith(
      expect.objectContaining({ addConnection: expect.objectContaining({ id: 'conn-新的' }) }),
    )
  })
})
