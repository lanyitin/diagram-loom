/**
 * 資源表單。
 *
 * # 為什麼從「參照欄位」開始補
 *
 * 這個表單一直沒有測試，而它溜掉的第一件事就是：「對應哪個外部系統」
 * 列出了**全部**系統，包括自家的。
 *
 * 自家系統是靠自己的 `Container` 部署的，沒有獨立的系統實例
 * （`loom_core::logical::Logical::external_systems`）。選了一個自家系統，
 * 使用者就建出一個模型上不該存在的東西——而 **lint 不會叫**，
 * 因為 L001 只走 `external_systems()`，根本看不到它。
 *
 * 一個「工具說沒問題、但東西是錯的」的狀態，對一個賣點是防漏的工具
 * 是最糟的那種 bug。所以它值得一條測試釘住。
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import ResourceForm from './ResourceForm.vue'
import { useProject } from '../lib/store'
import type { Resource, Snapshot } from '../lib/model'

vi.mock('../lib/bindings', () => ({ commands: {} }))

/** 一個自家系統、一個外部系統。 */
function fakeSnapshot(): Snapshot {
  return {
    root: '/x', findings: [], rows: [], dirty: false, undoLabel: null, redoLabel: null,
    matrix: { relationships: [], environments: [], cells: [] },
    project: {
      id: 'p', slug: 'f', name: 'n',
      logical: {
        people: [],
        systems: [
          { id: 's-shop', slug: 'shop', name: '訂單系統', external: false, endpoints: [] },
          { id: 's-sso', slug: 'sso', name: '單一登入', external: true, endpoints: [] },
          { id: 's-crm', slug: 'crm', name: '客戶關係', external: true, endpoints: [] },
        ],
        containers: [],
        relationships: [],
      },
      environments: [{ id: 'env-prod', slug: 'prod', name: '正式' }],
    },
  } as unknown as Snapshot
}

const blankSystemInstance: Resource = {
  systemInstance: {
    environment: 'env-prod',
    instance: { id: 'si-1', slug: '', system: '', endpoints: [] },
  },
} as unknown as Resource

describe('新增外部系統實體', () => {
  let store: ReturnType<typeof useProject>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.snapshot = fakeSnapshot()
    store.editingResource = {
      resource: blankSystemInstance,
      isNew: true,
      kind: '外部系統實體',
    }
  })

  /** 「對應哪個外部系統」那一格。依標籤找，不用位置。 */
  function systemPicker(w: ReturnType<typeof mount>) {
    const label = w.findAll('label').find((l) => l.text().includes('對應哪個外部系統'))
    expect(label, '找不到「對應哪個外部系統」').toBeTruthy()
    return label!.findComponent({ name: 'Picker' })
  }

  it('用會打字的 Picker，不是原生選單', () => {
    // 旁邊的「哪個服務」「跑在哪台機器上」都是 Picker。同一張表單裡
    // 兩種操作感，使用者只會覺得這個工具前後不一致。
    const w = mount(ResourceForm)
    expect(systemPicker(w).exists()).toBe(true)
  })

  it('只列外部系統，自家的不該出現', () => {
    // 選了自家系統就建出一個模型上不存在的東西，而 lint 不會叫。
    const w = mount(ResourceForm)
    const options = systemPicker(w).props('options') as { label: string }[]

    expect(options.map((o) => o.label).sort()).toEqual(['crm', 'sso'])
  })

  it('一個外部系統都沒有時，說清楚下一步在哪', () => {
    // 空的選單跟「壞掉了」長得一樣。
    store.snapshot!.project.logical.systems = [
      { id: 's-shop', slug: 'shop', name: '訂單系統', external: false, endpoints: [] },
    ] as never
    const w = mount(ResourceForm)

    expect(w.text()).toContain('還沒有')
    expect(w.text()).toContain('外部')
  })
})

describe('位址填得到', () => {
  let store: ReturnType<typeof useProject>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.snapshot = fakeSnapshot()
  })

  function open(resource: Resource) {
    store.editingResource = { resource, isNew: true, kind: '外部系統實體' }
    return mount(ResourceForm)
  }

  it('外部系統實體有「接點與位址」可以填', () => {
    // 清單上有一欄「位址」，卻沒有任何畫面填得了它——而 lint 會為此叫
    // （L001「外部系統在這個環境沒有指定位址」）。工具指著問題卻沒給
    // 任何辦法，比不叫還糟。
    const w = open(blankSystemInstance)

    expect(w.text()).toContain('接點與位址')
    expect(w.find('.sub .link').exists(), '要能加一個接點').toBe(true)
  })

  it('加一個接點就填得了 IP', async () => {
    const w = open(blankSystemInstance)
    await w.find('.sub .link').trigger('click')

    const boxes = w.findAll('.ep input')
    expect(boxes.length, '接點名稱與位址兩格').toBeGreaterThanOrEqual(2)
    await boxes[1]!.setValue('sso.corp.local:443')

    const draft = (w.vm as unknown as { editingInstance: { holder: { endpoints: { address: string }[] } } })
    expect(draft.editingInstance.holder.endpoints[0]!.address).toBe('sso.corp.local:443')
  })

  it('接點定義是從**對應的那個外部系統**身上找的', () => {
    // 這是兩種實體唯一的差別，也是最容易寫錯的地方：服務實體看它對應的
    // 服務，外部系統實體看它對應的系統（見 domain-model.md 的不對稱表）。
    store.snapshot!.project.logical.systems[1]!.endpoints = [
      { id: 'ed-sso-https', slug: 'https', protocol: 'tcp' },
    ] as never
    const resource = {
      systemInstance: {
        environment: 'env-prod',
        instance: { id: 'si-1', slug: 'sso-prod', system: 's-sso', endpoints: [] },
      },
    } as unknown as Resource

    const w = open(resource)
    const options = (w.vm as unknown as { defsOfOwner: { label: string }[] }).defsOfOwner

    expect(options.map((o) => o.label)).toEqual(['https'])
  })

  it('「刻意獨立」兩種實體都有', () => {
    // L008 對外部系統實體也會叫，也吃 standalone。只有一邊有的話，
    // 使用者只能繞去 lint 面板才關得掉。
    expect(open(blankSystemInstance).text()).toContain('刻意獨立')
  })
})

describe('選單的規矩', () => {
  /**
   * **從模型裡挑一個元素 → `Picker`；封閉列舉 → 原生 `<select>`。**
   *
   * 這條線是有理由的：模型裡的東西會長（一個環境幾百台機器），而且使用者
   * 手上通常已經有名字了——他要的是「打三個字就到」。列舉只有三五個而且
   * 永遠不會變長，套上打字搜尋只是多一層互動。
   *
   * 沒有這條測試的話，下一個欄位又會退回 `<select>`，而且沒有人會發現——
   * 這正是「對應哪個外部系統」當初溜掉的方式。同一張表單裡兩種操作感，
   * 使用者只會覺得這個工具前後不一致。
   */
  it('原生 select 只剩下封閉列舉那兩個', () => {
    const source = readFileSync(join(import.meta.dirname, 'ResourceForm.vue'), 'utf8')

    // 每個 <select> 綁的是哪個欄位。
    const bound = [...source.matchAll(/<select[^>]*read\('([^']+)'\)/g)].map((m) => m[1])

    expect(bound.sort()).toEqual(
      ['endpointDef.def.protocol', 'infraEndpoint.endpoint.protocol', 'node.node.kind'].sort(),
    )
    // 數量也要對得上，免得有人寫了一個沒綁 read() 的 select 溜過去。
    expect(source.match(/<select/g) ?? []).toHaveLength(3)
  })

  it('參照欄位都用 Picker', () => {
    const source = readFileSync(join(import.meta.dirname, 'ResourceForm.vue'), 'utf8')
    // 服務 × 1、接點定義 × 1、契約 × 3、機器 × 1、設備接點 × 1、
    // 服務實體 × 3、外部系統實體 × 1
    expect(source.match(/<Picker/g) ?? []).toHaveLength(11)
  })
})

describe('挑了接點定義', () => {
  let store: ReturnType<typeof useProject>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.snapshot = fakeSnapshot()
    store.snapshot!.project.logical.systems[1]!.endpoints = [
      { id: 'ed-jdbc', slug: 'sql', protocol: 'jdbc' },
      { id: 'ed-https', slug: 'https', protocol: 'tcp' },
    ] as never
    store.editingResource = {
      resource: {
        systemInstance: {
          environment: 'env-prod',
          instance: { id: 'si-1', slug: 'sso-prod', system: 's-sso', endpoints: [] },
        },
      } as unknown as Resource,
      isNew: true,
      kind: '外部系統實體',
    }
  })

  /** 新增一個接點，然後挑第 n 個定義。 */
  async function pick(w: ReturnType<typeof mount>, n: number) {
    await w.find('.sub .link').trigger('click')
    const picker = w.findAllComponents({ name: 'Picker' }).at(-1)!
    await picker.find('input').trigger('focus')
    await picker.findAll('.item')[n]!.trigger('click')
    return (w.vm as unknown as {
      editingInstance: { holder: { endpoints: { slug: string; def: string; protocol: string }[] } }
    }).editingInstance.holder.endpoints[0]!
  }

  it('協定要跟著換成那個定義的協定', async () => {
    // 不同步的話，一個對應到 JDBC 定義的接點會一直記著 tcp——存下來的資料
    // 跟它自己指的定義不一致。而且沒有畫面看得出來，只有匯入試算表的
    // 差異比對會印出來，所以會安靜地錯很久。
    const w = mount(ResourceForm)
    // 第 0 項是 allowEmpty 的「（沒對應到契約）」，所以 sql 是第 1 項。
    const endpoint = await pick(w, 1)

    expect(endpoint.def).toBe('ed-jdbc')
    expect(endpoint.protocol).toBe('jdbc')
  })

  it('名字空著就代填，打過的不動', async () => {
    const w = mount(ResourceForm)
    const endpoint = await pick(w, 1)
    expect(endpoint.slug).toBe('sql')

    // 改成自己的名字之後再挑別的定義，名字不該被蓋掉。
    endpoint.slug = '我自己取的'
    const picker = w.findAllComponents({ name: 'Picker' }).at(-1)!
    await picker.find('input').trigger('focus')
    await picker.findAll('.item')[2]!.trigger('click')

    expect(endpoint.slug).toBe('我自己取的')
    expect(endpoint.protocol).toBe('tcp')
  })

  it('清掉對應時不要動協定與名字', async () => {
    // 「沒對應到契約」是合法狀態。清掉的時候把協定重設會是另一種安靜的改動。
    const w = mount(ResourceForm)
    const endpoint = await pick(w, 1)

    const picker = w.findAllComponents({ name: 'Picker' }).at(-1)!
    await picker.find('input').trigger('focus')
    await picker.findAll('.item')[0]!.trigger('click')   // （沒對應到契約）

    expect(endpoint.def).toBeNull()
    expect(endpoint.protocol).toBe('jdbc')
    expect(endpoint.slug).toBe('sql')
  })
})

/**
 * 備註。
 *
 * # 這一組在守什麼
 *
 * 備註是**每一種資源都有**的欄位，Rust 存得下、表格看得到、MCP 也寫得進去
 * ——但這張表單裡一度**一個字都沒有**，所以人填不了。那種缺口沒有任何錯誤
 * 訊息，也沒有規則會叫（備註正是這個模型裡唯一沒有規則在看的欄位），
 * 只有使用者自己發現。
 *
 * # 為什麼下面那張表要**再抄一份**
 *
 * 直覺會想從 `ResourceForm.vue` 把 `MEMO_PATHS` 挖出來，測試就不必維護第二份。
 * 那樣是錯的：draft 也照著同一份路徑長出來的話，路徑抄錯時兩邊會**一起錯、
 * 一起綠**——測試變成「我跟我自己一樣」。實測過，把 `node.node.memo` 改成
 * `node.memo` 那條測試照樣是綠的。
 *
 * 所以這裡的 draft 照**真正的 `Resource` 形狀**寫（對照 `bindings.ts` 的
 * `Resource_Serialize`），期望路徑也獨立寫一份。兩份互相對照才有意義。
 */
describe('備註', () => {
  const source = () => readFileSync(join(import.meta.dirname, 'ResourceForm.vue'), 'utf8')

  /** 備註住的那個物件。形狀照 `bindings.ts`，不是照 `MEMO_PATHS`。 */
  const leaf = () => ({ id: 'x', slug: '', memo: '' })
  const held = () => ({ ...leaf(), standalone: false, endpoints: [] })
  const ENV = 'env-prod'

  /** [這是哪一種, 備註該落在哪, 一份形狀正確的 draft] */
  const CASES: [string, string, Resource][] = [
    ['person', 'person.memo', { person: leaf() }],
    ['system', 'system.memo', { system: { ...leaf(), external: true, endpoints: [] } }],
    ['container', 'container.memo', { container: { ...leaf(), system: '', endpoints: [] } }],
    ['endpointDef', 'endpointDef.def.memo', { endpointDef: { owner: '', def: leaf() } }],
    ['relationship', 'relationship.memo', { relationship: { ...leaf(), purpose: '' } }],
    ['environment', 'environment.memo', { environment: { ...leaf(), name: '' } }],
    ['node', 'node.node.memo', { node: { environment: ENV, within: null, node: leaf() } }],
    ['infra', 'infra.node.memo', { infra: { environment: ENV, node: leaf() } }],
    ['infraEndpoint', 'infraEndpoint.endpoint.memo',
      { infraEndpoint: { environment: ENV, node: '', endpoint: leaf() } }],
    ['instance', 'instance.instance.memo',
      { instance: { environment: ENV, node: '', instance: held() } }],
    ['systemInstance', 'systemInstance.instance.memo',
      { systemInstance: { environment: ENV, instance: held() } }],
  ] as unknown as [string, string, Resource][]

  /** 照著路徑從送出去的那份讀回來。 */
  function at(o: unknown, path: string): unknown {
    return path.split('.').reduce<unknown>((v, k) => (v as Record<string, unknown>)?.[k], o)
  }

  it.each(CASES)('%s 的備註要落在 %s', async (kind, path, resource) => {
    // 路徑抄錯（`node.memo` vs `node.node.memo`）的話，字會寫到外層那個包裝
    // 物件上——Rust 讀不到，而畫面上按了儲存什麼錯誤都沒有。
    setActivePinia(createPinia())
    const store = useProject()
    store.snapshot = fakeSnapshot()
    store.editingResource = { resource, isNew: false, kind }
    const applyEdit = vi.spyOn(store, 'applyEdit').mockResolvedValue()

    const w = mount(ResourceForm)
    const box = w.find('textarea')
    expect(box.exists(), `${kind} 沒有備註欄`).toBe(true)
    await box.setValue('等年底汰換')
    await w.find('button.primary').trigger('click')

    const sent = applyEdit.mock.calls[0]?.[0] as { updateResource?: unknown }
    expect(at(sent?.updateResource, path), `${kind} 的備註沒落在 ${path}`).toBe('等年底汰換')
  })

  it('十一種資源一種都不能少', () => {
    // 少一種的話，那一種就是「表格上看得到、MCP 寫得進去、人填不了」——
    // 沒有錯誤訊息。這條是「新加一種資源忘了補備註」每天真的會擋下來的那一層：
    // `Record<Kind, string>` 的型別檢查只在 `pnpm run build:web` 跑，
    // 而 `mise run check` 沒有型別檢查。
    const kinds = readFileSync(join(import.meta.dirname, '../lib/bindings.ts'), 'utf8')
      .match(/export type Kind = ([^;]+);/)![1]!
      .split('|')
      .map((s) => s.trim().replaceAll('"', ''))

    expect(CASES.map(([k]) => k).sort()).toEqual([...kinds].sort())
    // 表單那一份也要一種不少，否則會有「測試涵蓋到、表單沒有」的種類。
    const block = source().match(/const MEMO_PATHS[^{]*\{([\s\S]*?)\n\}/)
    expect(block, '找不到 MEMO_PATHS，是不是改名了？').not.toBeNull()
    const inForm = [...block![1]!.matchAll(/(\w+):\s*'/g)].map((m) => m[1]!)
    expect(inForm.sort()).toEqual([...kinds].sort())
  })

  it('備註只寫一次，不是十一個分支各抄一份', () => {
    // 抄開的那一天，漏掉的那一種不會壞掉，只是安靜地少一個欄位。
    expect(source().match(/<textarea/g) ?? []).toHaveLength(1)
  })
})

/**
 * 契約的目標接點。
 *
 * # 這一組在守什麼
 *
 * 這一欄一度綁在 `relationship.toEndpoint` 上，而 Rust 的欄位叫
 * `to_endpoint`（`bindings.ts` 的 `Relationship_Serialize`）。症狀是
 * **讀不到也寫不進**：本來就選好的接點在畫面上是空的，重新挑一個按了儲存
 * 也沒有任何錯誤訊息，因為那個值落在一個 Rust 根本不看的鍵上。
 *
 * 跟備註那一組是同一類錯——路徑抄錯，安靜地什麼都沒發生。
 */
describe('契約的目標接點', () => {
  let store: ReturnType<typeof useProject>

  /** 一條指著外部系統 `sso` 的契約，現在選的是 `sql`。 */
  const contract = () => ({
    relationship: {
      id: 'r-1',
      slug: 'shop-連-sso',
      purpose: '登入',
      from: { system: 's-shop' },
      to: { system: 's-sso' },
      to_endpoint: 'ed-jdbc',
      memo: '',
    },
  }) as unknown as Resource

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.snapshot = fakeSnapshot()
    store.snapshot!.project.logical.systems[1]!.endpoints = [
      { id: 'ed-jdbc', slug: 'sql', protocol: 'jdbc' },
      { id: 'ed-https', slug: 'https', protocol: 'tcp' },
    ] as never
    store.editingResource = { resource: contract(), isNew: false, kind: 'relationship' }
  })

  /** 「連到目標的哪個接點」那一格。依標籤找，不用位置。 */
  function endpointPicker(w: ReturnType<typeof mount>) {
    const label = w.findAll('label').find((l) => l.text().includes('連到目標的哪個接點'))
    expect(label, '找不到「連到目標的哪個接點」').toBeTruthy()
    return label!.findComponent({ name: 'Picker' })
  }

  it('打開時要顯示現在選的那個接點', () => {
    // 空白跟「還沒選」長得一樣，使用者會以為資料掉了。
    const w = mount(ResourceForm)

    expect(endpointPicker(w).props('modelValue')).toBe('ed-jdbc')
  })

  it('改選另一個接點，送出時要落在 to_endpoint 上', async () => {
    const w = mount(ResourceForm)
    const applyEdit = vi.spyOn(store, 'applyEdit').mockResolvedValue()

    const picker = endpointPicker(w)
    await picker.find('input').trigger('focus')
    await picker.findAll('.item')[1]!.trigger('click')   // https
    await w.find('button.primary').trigger('click')

    const sent = applyEdit.mock.calls[0]?.[0] as {
      updateResource?: { relationship: Record<string, unknown> }
    }
    expect(sent?.updateResource?.relationship.to_endpoint).toBe('ed-https')
    // 落在別的鍵上就是「按了沒反應」——這裡把它釘死。
    expect(Object.keys(sent!.updateResource!.relationship)).not.toContain('toEndpoint')
  })
})

/**
 * 欄位路徑的大小寫。
 *
 * # 為什麼這條值得存在
 *
 * `bindings.ts` 裡只有 **`Resource` 的種類名**是 camelCase
 * （`endpointDef`、`infraEndpoint`、`systemInstance`），那是 enum 的
 * variant 名；**欄位一律 snake_case**（`to_endpoint`、`memo`、`protocol`）。
 *
 * 寫成 camelCase 的欄位不會編不過，也不會有錯誤訊息——`write()` 會開一個新的鍵，
 * Rust 安靜地忽略它。已經發生過兩次（`toEndpoint`），所以用一條規則擋掉一整類。
 */
describe('欄位路徑', () => {
  it('第一段以外都不准出現大寫', () => {
    const source = readFileSync(join(import.meta.dirname, 'ResourceForm.vue'), 'utf8')
    const paths = [...source.matchAll(/\b(?:read|write)\('([^']+)'/g)].map((m) => m[1]!)

    expect(paths.length, '一條 read()/write() 都沒抓到，是不是改寫法了？').toBeGreaterThan(10)
    const bad = paths.filter((p) => p.split('.').slice(1).some((seg) => /[A-Z]/.test(seg)))
    expect(bad, `這些路徑的欄位名不是 snake_case：${bad.join('、')}`).toEqual([])
  })
})

/**
 * 設備接點（VIP）。
 *
 * # 這一組在守什麼
 *
 * 這張表單一度有「名稱」與「位址」，卻沒有「掛在哪台設備上」——而
 * `Resource::InfraEndpoint` 的 `node` 是必填的，Rust 收到空的會回
 * `NoSuchSubject`。加上當時分頁列根本沒有「設備接點」這一頁，
 * 結果是 **VIP 完全建不出來**。
 *
 * 而建不出 VIP 的後果隔了三層才顯現：`connect::choices` 列的是設備身上的
 * VIP，所以一台沒有 VIP 的 F5 在「補一條連線」的目標選單裡是看不見的。
 * 使用者看到的是「選單裡沒有 F5」。
 */
describe('設備接點', () => {
  let store: ReturnType<typeof useProject>

  const blank = () => ({
    infraEndpoint: {
      environment: 'env-prod',
      node: '',
      endpoint: { id: 'ep-1', slug: '', def: null, protocol: 'tcp', address: null, memo: '' },
    },
  }) as unknown as Resource

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.snapshot = fakeSnapshot()
    store.snapshot!.project.environments[0]!.infra = [
      { id: 'inf-f5', slug: 'echeck-f5', endpoints: [] },
    ] as never
    store.editingResource = { resource: blank(), isNew: true, kind: '設備接點' }
  })

  /** 「掛在哪台設備上」那一格。依標籤找，不用位置。 */
  function nodePicker(w: ReturnType<typeof mount>) {
    const label = w.findAll('label').find((l) => l.text().includes('掛在哪台設備上'))
    expect(label, '找不到「掛在哪台設備上」').toBeTruthy()
    return label!.findComponent({ name: 'Picker' })
  }

  it('選得到這個環境裡的設備', () => {
    // 沒有這一格的話 `node` 永遠是空字串，Rust 回 NoSuchSubject——
    // 一張填得完卻永遠存不進去的表單。
    const w = mount(ResourceForm)

    expect((nodePicker(w).props('options') as { label: string }[]).map((o) => o.label))
      .toEqual(['echeck-f5'])
  })

  it('位址與協定都填得了，送出時整份帶著走', async () => {
    const w = mount(ResourceForm)
    const applyEdit = vi.spyOn(store, 'applyEdit').mockResolvedValue()

    const picker = nodePicker(w)
    await picker.find('input').trigger('focus')
    await picker.findAll('.item')[0]!.trigger('click')

    // 依標籤找，不用位置——Picker 自己也有一個輸入框。
    const box = (text: string) =>
      w.findAll('label').find((l) => l.text().includes(text))!.find('input')
    await box('名稱').setValue('vip-echeck')
    await box('位址').setValue('10.0.0.100:8443')
    await w.find('select').setValue('jdbc')
    await w.find('button.primary').trigger('click')

    const sent = (applyEdit.mock.calls[0]?.[0] as { addResource?: Record<string, never> })
      ?.addResource?.infraEndpoint as unknown as {
        node: string
        endpoint: { slug: string; address: string; protocol: string }
      }
    expect(sent.node).toBe('inf-f5')
    expect(sent.endpoint.slug).toBe('vip-echeck')
    expect(sent.endpoint.address).toBe('10.0.0.100:8443')
    expect(sent.endpoint.protocol).toBe('jdbc')
  })

  it('這個環境一台設備都沒有時，說清楚下一步在哪', () => {
    // 空的選單跟「壞掉了」長得一樣。
    store.snapshot!.project.environments[0]!.infra = [] as never
    const w = mount(ResourceForm)

    expect(w.text()).toContain('還沒有任何設備')
  })
})
