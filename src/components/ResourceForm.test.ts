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
      ['endpointDef.def.protocol', 'node.node.kind'].sort(),
    )
    // 數量也要對得上，免得有人寫了一個沒綁 read() 的 select 溜過去。
    expect(source.match(/<select/g) ?? []).toHaveLength(2)
  })

  it('參照欄位都用 Picker', () => {
    const source = readFileSync(join(import.meta.dirname, 'ResourceForm.vue'), 'utf8')
    // 服務 × 1、接點定義 × 1、契約 × 3、機器 × 1、服務實體 × 3、外部系統實體 × 1
    expect(source.match(/<Picker/g) ?? []).toHaveLength(10)
  })
})
