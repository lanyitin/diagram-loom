/**
 * 標註面板。
 *
 * 這裡真正在守的是三件事：
 *
 * 1. **沒指定的一個都不能漏掉**——連沒有文字的形狀也要列。這個工具的命是怕漏，
 *    自己決定「這個不用管」，使用者就永遠不知道有這回事。
 * 2. **建議不會自己套用**。綁定之後對帳就把它當事實，而錯的那一項不會有人
 *    再檢查它。
 * 3. **線只能指給連線**。一個方框指成一條連線，對帳會永遠對不上它。
 */

import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import DiagramBinder from './DiagramBinder.vue'
import type { Annotation, UnboundShape } from '../lib/model'

const SHAPES: UnboundShape[] = [
  { cell: 'a', label: 'Apache 叢集', edge: false },
  { cell: 'b', label: '', edge: false },
  { cell: 'line', label: '查快取', edge: true },
]

const ANNOTATION: Annotation = {
  shapes: [
    { id: 'i-apache', kind: 'container-instance', label: 'apache-01' },
    { id: 'n-vm', kind: 'deployment-node', label: 'vm-01' },
  ],
  connections: [{ id: 'conn-1', kind: 'connection', label: '查快取' }],
  guesses: [{ cell: 'a', target: 'i-apache' }],
}

function binder(over: Record<string, unknown> = {}) {
  return mount(DiagramBinder, {
    props: { shapes: SHAPES, annotation: ANNOTATION, assigned: [], spot: null, ...over },
  })
}

/** 展開第 n 列的選單，讀出裡面有哪些選項。 */
async function optionsOf(w: ReturnType<typeof binder>, row: number) {
  await w.findAll('.picker input')[row]!.trigger('focus')
  return w.findAll('.item .label').map((e) => e.text())
}

describe('清單', () => {
  it('每一個沒指定的形狀都列得出來', () => {
    expect(binder().findAll('.list > li')).toHaveLength(3)
  })

  it('沒有文字的形狀照樣列，只是說它沒有文字', () => {
    expect(binder().text()).toContain('（沒有文字）')
  })

  it('還剩幾個講在最上面', () => {
    // 清單天生會說「還剩幾個」——這正是它比在圖上一個一個點更好的地方。
    expect(binder().find('.left').text()).toBe('還剩 3 個')
  })

  it('都指定完了要說一聲', () => {
    // 空的清單跟「壞掉了」長得一樣。
    expect(binder({ shapes: [] }).text()).toContain('都指定過了')
  })

  it('沒指定的形狀對帳看不見，這件事要寫在畫面上', () => {
    // 不講的話，使用者會以為「圖上有畫」就等於「這件事管到了」。
    expect(binder().find('.foot').text()).toContain('對帳看不見')
  })
})

describe('可以指給誰', () => {
  it('框只列得到元素', async () => {
    expect(await optionsOf(binder(), 0)).toEqual(['apache-01', 'vm-01'])
  })

  it('線只列得到連線', async () => {
    // 混在一起的話，一個方框可以被指成一條連線——對帳會永遠對不上它。
    expect(await optionsOf(binder(), 2)).toEqual(['查快取'])
  })

  it('選一個就送出指定', async () => {
    const w = binder()
    await w.findAll('.picker input')[0]!.trigger('focus')
    await w.findAll('.item')[1]!.trigger('click')
    expect(w.emitted('assign')).toEqual([['a', 'n-vm']])
  })
})

describe('建議', () => {
  it('猜得到的那一個給一顆按鈕', () => {
    expect(binder().find('.guess').text()).toContain('apache-01')
  })

  it('猜不到的就沒有按鈕', () => {
    // 名字對不上時給一個建議，比不給更糟：人會直接按下去。
    expect(binder({ annotation: { ...ANNOTATION, guesses: [] } }).find('.guess').exists()).toBe(false)
  })

  it('建議不會自己套用，要有人按', async () => {
    const w = binder()
    expect(w.emitted('assign')).toBeUndefined()
    await w.find('.guess').trigger('click')
    expect(w.emitted('assign')).toEqual([['a', 'i-apache']])
  })
})

describe('標示與收回', () => {
  it('按一下標示，圖上就只亮那一個', async () => {
    // 清單上 47 個名字，光靠名字對不出來——而對不出來的人就會亂指。
    const w = binder()
    await w.find('.mark').trigger('click')
    expect(w.emitted('spot')).toEqual([['a']])
  })

  it('再按一次就熄掉', async () => {
    const w = binder({ spot: 'a' })
    await w.find('.mark').trigger('click')
    expect(w.emitted('spot')).toEqual([[null]])
  })

  it('指定過的收得回來', async () => {
    // 指錯了一定要收得回來：綁定之後對帳就把它當事實。
    const w = binder({ assigned: [{ cell: 'a', label: 'Apache 叢集', target: 'i-apache' }] })
    expect(w.find('.undo').text()).toContain('apache-01')
    await w.find('.undo .link').trigger('click')
    expect(w.emitted('unassign')).toEqual([['a']])
  })
})

/**
 * 簡圖的標註對象。
 *
 * 使用者自己畫的圖不一定是部署圖——Context 圖畫人與系統、Container 圖畫
 * 服務與契約，兩種還可能混在同一張。在這之前那些框**沒有任何東西可以指**，
 * 而畫面上沒有任何訊息說明為什麼，使用者只會覺得選單壞了。
 */
describe('邏輯層的選項', () => {
  const LOGICAL: Annotation = {
    shapes: [
      { id: 'p-customer', kind: 'person', label: 'customer' },
      { id: 's-sso', kind: 'software-system', label: 'sso' },
      { id: 'c-redis', kind: 'container', label: 'redis' },
      { id: 'i-apache', kind: 'container-instance', label: 'apache-01' },
    ],
    connections: [
      { id: 'r-cache', kind: 'relationship', label: 'api-連-redis' },
      { id: 'conn-1', kind: 'connection', label: '查快取' },
    ],
    guesses: [],
  }

  /** 展開第 n 列的選單，讀出分組的標題。 */
  async function groupsOf(w: ReturnType<typeof binder>, row: number) {
    await w.findAll('.picker input')[row]!.trigger('focus')
    return w.findAll('.group').map((e) => e.text())
  }

  it('人指得到', async () => {
    // 這是整組的理由：Context 圖上那個小人以前指不到任何東西。
    const w = binder({ annotation: LOGICAL })
    expect(await optionsOf(w, 0)).toContain('customer')
  })

  it('分組的標題是中文，不是 kebab-case 原文', async () => {
    // 漏掉一種的話標題會變成 `software-system`——那看起來像資料髒掉，
    // 不像少寫了一行。型別上由 `Record<ElementKind, string>` 擋，這裡守畫面。
    const w = binder({ annotation: LOGICAL })
    const groups = await groupsOf(w, 0)
    expect(groups).toContain('人')
    expect(groups).toContain('系統')
    expect(groups).toContain('服務')
    expect(groups.some((g) => g.includes('-'))).toBe(false)
  })

  it('契約只出現在線那一列，不出現在框那一列', async () => {
    // 契約是邏輯層的**線**。放進框的選單裡，使用者就能把一個方框指成
    // 一條契約——那在對帳時會變成一個永遠對不上的東西。
    const w = binder({ annotation: LOGICAL })
    expect(await optionsOf(w, 0)).not.toContain('api-連-redis')

    const line = binder({ annotation: LOGICAL })
    expect(await optionsOf(line, 2)).toContain('api-連-redis')
  })
})
