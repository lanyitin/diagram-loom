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
  group: 'logical',
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
  group: 'environment',
  columns: ['名稱', '種類', '上面跑的服務'],
  environment: 'env-prod',
  emptyHint: '站點、實體機、VM、Linux 容器都是機器。',
  rows: [
    { id: 'n-site', cells: ['dc-main', '站點', ''], depth: 0, severity: null, resource: {} },
    { id: 'n-vm', cells: ['vm-01', '虛擬機', 'redis-01'], depth: 1, severity: null, resource: {} },
  ],
} as unknown as Table

const environmentTable: Table = {
  kind: 'environment',
  title: '環境',
  group: 'project',
  columns: ['名稱', '顯示名', '機器'],
  environment: null,
  emptyHint: '至少要有一個環境，lint 才有話說。',
  rows: [
    {
      id: 'env-prod',
      cells: ['prod', '正式環境', '3'],
      depth: 0,
      severity: null,
      resource: { environment: { id: 'env-prod', slug: 'prod' } },
    },
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

/**
 * 欄位偏好會寫進 localStorage，而 vitest 同一個檔案共用一份。
 * 不清的話，某個測試藏掉的欄會跟著跑到下一個測試——症狀是那一欄
 * 「莫名其妙不見了」，而排序看起來像壞掉。
 */
beforeEach(() => localStorage.clear())

describe('資源檢視', () => {
  let store: ReturnType<typeof useProject>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.snapshot = fakeSnapshot()
    vi.mocked(commands.resourceTables).mockResolvedValue({
      status: 'ok', data: [environmentTable, containerTable, nodeTable],
    } as never)
  })

  async function openIt() {
    const w = mount(ResourceView)
    await flushPromises()
    return w
  }

  it('照 Rust 給的欄位畫，前端不認得那些欄位', async () => {
    const w = await openIt()
    // 只看真正的資料欄。最後那格是「欄位」選單，不是欄位。
    const headers = w.findAll('thead th[data-column]').map((t) => t.text())
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

  it('分頁分成母版與分身兩組', async () => {
    // 這個分界是整個領域模型最重要的一件事。畫成同一串等於在跟使用者說
    // 「這些都差不多」。
    const w = await openIt()
    const groups = w.findAll('.tabs .group').map((g) => g.text())
    expect(groups).toEqual(['邏輯層 · 母版', '環境層 · 分身'])
  })

  it('「環境」不在分頁列上', async () => {
    // 它列的是全部環境，不隸屬於任何一個。使用者要找它的時候會去按
    // 環境選單，所以它就開在那裡。
    const w = await openIt()
    const titles = w.findAll('.tabs > button').map((b) => b.text())
    expect(titles.some((t) => t.startsWith('環境'))).toBe(false)
  })

  it('「管理環境…」開出來的就是環境那張表', async () => {
    const w = await openIt()
    await w.find('.pick').trigger('click')
    await w.find('.menu .manage').trigger('click')

    const dialog = w.find('[aria-label="管理環境"]')
    expect(dialog.exists()).toBe(true)
    expect(dialog.text()).toContain('prod')
    expect(dialog.text()).toContain('正式環境')
  })

  it('在邏輯層那一頁時，選單不列環境', async () => {
    // 邏輯層是母版，跟環境無關。列出來只會讓人以為這一頁的內容會跟著變。
    store.snapshot!.project.environments = [
      { id: 'env-prod', slug: 'prod', name: '正式' },
      { id: 'env-test', slug: 'test', name: '測試' },
    ] as never
    const w = await openIt()
    await w.find('.pick').trigger('click')

    // 只剩「管理環境…」——它跟看哪個環境無關，任何時候都要拿得到。
    expect(w.findAll('.menu button').map((b) => b.text())).toEqual(['管理環境…'])
  })

  it('切到環境層的分頁之後才問是哪個環境', async () => {
    store.snapshot!.project.environments = [
      { id: 'env-prod', slug: 'prod', name: '正式' },
      { id: 'env-test', slug: 'test', name: '測試' },
    ] as never
    const w = await openIt()
    await w.findAll('.tabs > button')[1]!.trigger('click')   // 機器
    await w.find('.pick').trigger('click')

    const items = w.findAll('.menu button')
    expect(items.map((b) => b.text().replace('✓', '').trim()))
      .toEqual(['prod', 'test', '管理環境…'])
    // 現在停在哪一個要看得出來，不然選單只是一份清單。
    expect(items[0]!.find('.tick').text()).toBe('✓')
    expect(items[1]!.find('.tick').text()).toBe('')
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

describe('排序與搜尋', () => {
  let store: ReturnType<typeof useProject>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.snapshot = fakeSnapshot()
    vi.mocked(commands.resourceTables).mockResolvedValue({
      status: 'ok', data: [environmentTable, containerTable, nodeTable],
    } as never)
  })

  async function openIt() {
    const w = mount(ResourceView)
    await flushPromises()
    return w
  }

  const firstColumn = (w: ReturnType<typeof mount>) =>
    w.findAll('tbody tr').map((r) => r.findAll('td')[1]!.text())

  it('點欄位標題會排序', async () => {
    const w = await openIt()
    expect(firstColumn(w)).toEqual(['redis', 'order-api'])

    await w.findAll('thead th.sortable')[0]!.trigger('click')
    expect(firstColumn(w)).toEqual(['order-api', 'redis'])
  })

  it('再點一次倒過來，第三次回到原始順序', async () => {
    // 一定要有辦法回到原始順序：Rust 給的順序是有意義的。
    const w = await openIt()
    const header = () => w.findAll('thead th.sortable')[0]!

    await header().trigger('click')
    await header().trigger('click')
    expect(firstColumn(w)).toEqual(['redis', 'order-api'])
    await header().trigger('click')
    expect(firstColumn(w)).toEqual(['redis', 'order-api'])
    expect(header().attributes('aria-sort')).toBe('none')
  })

  it('排序狀態讓螢幕閱讀器讀得到，而不是只有一個箭頭', async () => {
    const w = await openIt()
    await w.findAll('thead th.sortable')[0]!.trigger('click')
    expect(w.findAll('thead th.sortable')[0]!.attributes('aria-sort')).toBe('ascending')
  })

  it('換一頁就把排序重設', async () => {
    // 欄位不一樣，沿用上一頁的「第幾欄」沒有意義。
    const w = await openIt()
    await w.findAll('thead th.sortable')[0]!.trigger('click')
    await w.findAll('.tabs button')[1]!.trigger('click')
    expect(w.findAll('thead th.sortable')[0]!.attributes('aria-sort')).toBe('none')
  })

  it('藏起來的欄不畫，但資料沒有跟著位移', async () => {
    // 這是欄位可以藏之後最容易搞錯的地方：畫面上的第 N 欄
    // 不再等於 `cells` 裡的第 N 格。
    const w = await openIt()
    await w.find('thead .cols').trigger('click')

    // 選單裡的順序就是資料的順序：名稱、顯示名、所屬系統、prod
    await w.findAll('.pop button')[1]!.trigger('click')   // 關掉「顯示名」

    expect(w.findAll('thead th[data-column]').map((t) => t.text()))
      .toEqual(['名稱', '所屬系統', 'prod'])
    // 「所屬系統」那格要還是 shop，不是被擠成「Redis 快取」。
    expect(w.findAll('tbody tr')[0]!.findAll('td').map((t) => t.text()))
      .toEqual(['', 'redis', 'shop', '6 台', '✎✕'])
  })

  it('第一欄不給關', async () => {
    const w = await openIt()
    await w.find('thead .cols').trigger('click')
    expect(w.findAll('.pop button')[0]!.attributes('disabled')).toBeDefined()
    expect(w.findAll('.pop button')[0]!.text()).toContain('固定')
  })

  it('藏掉正在排序的那一欄，排序也一起收掉', async () => {
    // 不收的話列還是照那一欄排，但畫面上沒有任何東西能解釋這個順序——
    // 沒有欄名、沒有箭頭，看起來就是列莫名其妙亂掉了。
    const w = await openIt()
    // 點兩次是遞減。遞增排出來剛好跟原始順序一樣，那樣就分不出
    // 到底有沒有在排序了。
    await w.findAll('thead th[data-column]')[1]!.trigger('click')   // 依「顯示名」遞增
    await w.findAll('thead th[data-column]')[1]!.trigger('click')   // 遞減
    expect(firstColumn(w)).toEqual(['order-api', 'redis'])

    await w.find('thead .cols').trigger('click')
    await w.findAll('.pop button')[1]!.trigger('click')             // 把那一欄關掉

    expect(firstColumn(w)).toEqual(['redis', 'order-api'])
  })

  it('上面那個搜尋框對資源也有效', async () => {
    store.search = 'order'
    const w = await openIt()
    expect(firstColumn(w)).toEqual(['order-api'])
  })

  it('篩到一列都不剩時，說清楚不是東西不見了', async () => {
    // 「本來就是空的」跟「被你篩掉了」是兩件事。混在一起會讓人以為資料掉了。
    store.search = '沒有這種東西'
    const w = await openIt()
    expect(w.find('.empty').text()).toContain('本來有 2 列')
  })
})

/** 分頁列裡也有「＋ 新增」那顆，所以要照名字挑，不能用位置。 */
function connectionTab(w: ReturnType<typeof mount>) {
  return w.findAll('.tabs > button').find((b) => b.text().startsWith('連線'))!
}

describe('連線分頁', () => {
  let store: ReturnType<typeof useProject>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.snapshot = fakeSnapshot()
    vi.mocked(commands.resourceTables).mockResolvedValue({
      status: 'ok', data: [environmentTable, containerTable, nodeTable],
    } as never)
  })

  async function openIt() {
    const w = mount(ResourceView)
    await flushPromises()
    return w
  }

  it('連線是最後一個分頁', async () => {
    const w = await openIt()
    expect(connectionTab(w).exists()).toBe(true)
  })

  it('預設不是停在連線那一頁', async () => {
    // 從零開始的人要先建服務與契約，連線是後面的事。
    const w = await openIt()
    expect(w.findComponent({ name: 'ConnectionTable' }).exists()).toBe(false)
  })

  it('新增連線先問是實現哪一條契約', async () => {
    // 沒有契約的連線 lint 會叫（L003），所以這裡不給「不選」這個選項。
    store.snapshot!.project.logical.relationships = [
      { id: 'r-1', slug: 'app-連-redis' },
    ] as never
    const w = await openIt()
    await connectionTab(w).trigger('click')
    await w.find('button.add').trigger('click')

    expect(w.find('[role="dialog"]').exists()).toBe(true)
    await w.find('.box .primary').trigger('click')
    expect(store.addingConnection).toEqual({
      environment: 'env-prod', relationship: 'r-1', label: 'app-連-redis',
    })
  })

  it('一條契約都沒有時，說要先去哪裡建', async () => {
    // 給一個空下拉選單等於讓人卡住，而且看不出卡在哪。
    const w = await openIt()
    await connectionTab(w).trigger('click')
    await w.find('button.add').trigger('click')
    expect(w.find('.box').text()).toContain('還沒有')
    expect(w.find('.box .primary').exists()).toBe(false)
  })
})


/**
 * 剛開的新專案：**零個環境**。
 *
 * 這裡守的是一個會把人卡死的迴圈：新增第一個環境的唯一入口是「環境 ▾ →
 * 管理環境…」，而那顆鈕本來寫著 `v-if="store.environments.length"`
 * ——要先有環境才能新增環境。畫面上不會有任何錯誤訊息，只是找不到那顆鈕。
 */
describe('一個環境都還沒有的新專案', () => {
  const emptyEnvironments: Table = {
    ...environmentTable,
    rows: [],
  } as unknown as Table

  beforeEach(() => {
    setActivePinia(createPinia())
    const store = useProject()
    const snap = fakeSnapshot() as unknown as { project: { environments: unknown[] } }
    snap.project.environments = []
    store.snapshot = snap as unknown as Snapshot
    vi.mocked(commands.resourceTables).mockResolvedValue({
      status: 'ok', data: [emptyEnvironments, containerTable],
    } as never)
  })

  async function openIt() {
    const w = mount(ResourceView)
    await flushPromises()
    return w
  }

  it('環境那顆鈕還在，不然新增第一個環境就沒有入口了', async () => {
    const w = await openIt()
    expect(w.find('.env .pick').exists()).toBe(true)
  })

  it('鈕上直接說「還沒有」，不是只寫「環境」', async () => {
    // 只寫「環境」看起來像一切正常，而使用者正卡在「那我要去哪裡建」。
    const w = await openIt()
    expect(w.find('.env .pick').text()).toContain('還沒有')
  })

  it('點開之後「管理環境…」按得到', async () => {
    const w = await openIt()
    await w.find('.env .pick').trigger('click')
    const manage = w.findAll('.env .menu button').find((b) => b.text().includes('管理環境'))
    expect(manage).toBeTruthy()

    await manage!.trigger('click')
    const add = w.findAll('button').find((b) => b.text().includes('新增環境'))
    expect(add?.attributes('disabled')).toBeUndefined()
  })

  it('新增連線時講出「還沒有環境」，而不是按了沒反應', async () => {
    // 「下一步」在沒有環境時會安靜地 return——按鈕什麼都不做是最難查的那種。
    const store = useProject()
    const snap = store.snapshot as unknown as {
      project: { logical: { relationships: unknown[] } }
    }
    snap.project.logical.relationships = [{ id: 'r1', slug: 'a-to-b' }]

    const w = await openIt()
    const connections = w.findAll('.tabs button').find((b) => b.text().includes('連線'))
    await connections!.trigger('click')
    await w.findAll('button').find((b) => b.text().includes('新增'))!.trigger('click')

    // 只看對話框裡的字。看整頁的話，環境那顆鈕上的「還沒有」會讓這條
    // 測試無論如何都綠——一條永遠會過的測試比沒有測試更糟。
    const dialog = w.find('[role="dialog"]')
    expect(dialog.exists()).toBe(true)
    expect(dialog.text()).toContain('這個專案還沒有')
    expect(dialog.text()).toContain('管理環境')
    expect(dialog.findAll('button').some((b) => b.text() === '下一步')).toBe(false)
  })
})
