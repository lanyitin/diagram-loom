/**
 * 資源檢視。
 *
 * 守的是兩件事：
 * ① 表格照 Rust 給的欄位畫，**前端不知道每種資源有哪些欄位**。
 * ② 空表要說「這是什麼」，不是留一片白——第一次用的人正是從這裡開始。
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import ResourceView from './ResourceView.vue'
import { useProject } from '../lib/store'
import { commands } from '../lib/bindings'
import type { Snapshot, Table } from '../lib/model'

vi.mock('../lib/bindings', () => ({
  commands: { resourceTables: vi.fn(), blankResource: vi.fn() },
}))

const containerTable: Table = {
  kind: 'container',
  title: '服務',
  columns: ['名稱', '顯示名', '所屬系統', 'prod'],
  environment: null,
  emptyHint: '還沒有任何服務。Redis、Consul 這種會接收請求的程序都算。',
  rows: [
    {
      id: 'c-redis',
      cells: ['redis', 'Redis 快取', 'shop', '6 台'],
      depth: 0,
      severity: null,
      resource: { container: { id: 'c-redis', slug: 'redis' } },
    },
    {
      id: 'c-api',
      cells: ['order-api', '訂單 API', 'shop', '—'],
      depth: 0,
      severity: 'error',
      resource: { container: { id: 'c-api', slug: 'order-api' } },
    },
  ],
} as unknown as Table

const nodeTable: Table = {
  kind: 'node',
  title: '機器',
  columns: ['名稱', '種類', '上面跑的服務'],
  environment: 'env-prod',
  emptyHint: '站點、實體機、VM、Linux 容器都是機器。',
  rows: [
    { id: 'n-site', cells: ['dc-main', '站點', ''], depth: 0, severity: null, resource: {} },
    { id: 'n-vm', cells: ['vm-01', '虛擬機', 'redis-01'], depth: 1, severity: null, resource: {} },
  ],
} as unknown as Table

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

describe('資源檢視', () => {
  let store: ReturnType<typeof useProject>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.snapshot = fakeSnapshot()
    vi.mocked(commands.resourceTables).mockResolvedValue({
      status: 'ok', data: [containerTable, nodeTable],
    } as never)
  })

  async function openIt() {
    const w = mount(ResourceView)
    await flushPromises()
    return w
  }

  it('照 Rust 給的欄位畫，前端不認得那些欄位', async () => {
    const w = await openIt()
    const headers = w.findAll('thead th').map((t) => t.text()).filter(Boolean)
    expect(headers).toEqual(['名稱', '顯示名', '所屬系統', 'prod'])
    expect(w.findAll('tbody tr')).toHaveLength(2)
    expect(w.findAll('tbody tr')[0]!.text()).toContain('6 台')
  })

  it('分頁上帶著每種資源的數量', async () => {
    const w = await openIt()
    const tabs = w.findAll('.tabs > button').map((b) => b.text())
    expect(tabs[0]).toContain('服務')
    expect(tabs[0]).toContain('2')
    expect(tabs[1]).toContain('機器')
  })

  it('有 lint 問題的列標出來', async () => {
    const w = await openIt()
    expect(w.findAll('.dot.error')).toHaveLength(1)
  })

  it('巢狀的機器用縮排表示層級', async () => {
    // 表格畫不出樹，但縮排看得出「這台在那個站點底下」。
    const w = await openIt()
    await w.findAll('.tabs > button')[1]!.trigger('click')

    const secondRow = w.findAll('tbody tr')[1]!.findAll('td')[1]!
    expect(secondRow.attributes('style')).toContain('padding-left')
  })

  it('空表說一句話，不是留一片白', async () => {
    // 第一次用的人正是從空表開始。留白的話他不知道下一步是什麼。
    vi.mocked(commands.resourceTables).mockResolvedValue({
      status: 'ok',
      data: [{ ...containerTable, rows: [] }],
    } as never)
    const w = await openIt()

    expect(w.find('table').exists()).toBe(false)
    expect(w.find('.empty').text()).toContain('Redis、Consul')
  })

  it('新增時跟 Rust 要一份空白的，不是自己拼', async () => {
    // 空白資源的 id 要在 Rust 發好，apply 才是決定性的。
    vi.mocked(commands.blankResource).mockResolvedValue({
      status: 'ok', data: { container: { id: '新的', slug: '' } },
    } as never)
    const w = await openIt()

    await w.find('.add').trigger('click')
    await flushPromises()

    expect(commands.blankResource).toHaveBeenCalledWith('container', null, null)
    expect(store.editingResource?.isNew).toBe(true)
    expect(store.editingResource?.kind).toBe('服務')
  })

  it('編輯直接拿那一列帶著的 resource 當起點', async () => {
    // 讓前端從 project 裡自己撈的話，要知道每種資源住在哪一層——那是模型知識。
    const w = await openIt()
    await w.findAll('tbody .icon')[0]!.trigger('click')

    expect(store.editingResource?.isNew).toBe(false)
    expect(store.editingResource?.resource).toEqual(containerTable.rows[0]!.resource)
  })

  it('刪除走的是同一個確認框，會先算影響', async () => {
    const w = await openIt()
    await w.findAll('tbody .icon.del')[0]!.trigger('click')

    expect(store.deleting).toEqual({
      edit: { deleteResource: containerTable.rows[0]!.resource },
      kind: '服務',
      label: 'redis',
    })
  })

  it('專案一改就重新取，表格不會跟 lint 說不一樣的話', async () => {
    await openIt()
    vi.mocked(commands.resourceTables).mockClear()

    store.snapshot = fakeSnapshot()
    await flushPromises()

    expect(commands.resourceTables).toHaveBeenCalled()
  })
})
