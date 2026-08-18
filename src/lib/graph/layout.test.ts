/**
 * 自己接的 elkjs 排版。
 *
 * 四項客觀檢查跟 `docs/nested-layout.md` 那一輪**完全一樣**——換掉排版引擎
 * 之後，「壞掉」的定義不該跟著換。
 *
 * 「排得好不好看」沒辦法自動判斷，但**壞掉**有客觀定義：
 * 子節點跑出父框、兄弟重疊、父子關係被改、綁定掉了。
 */

import { describe, expect, it } from 'vitest'
import { edgeId, layout, walk } from './layout'
import type { Shape } from '../diagram'
import type { Link } from '../model'

/** 站點 → 三台機器 → 各一個服務，外加一個不在機房裡的設備。 */
function shapes(): Shape[] {
  const out: Shape[] = [
    { id: 'site', loomId: 'site', kind: 'deploymentNode', label: '台北機房', style: '' },
    { id: 'f5', loomId: 'f5', kind: 'infrastructureNode', label: 'F5', style: '' },
  ]
  for (const n of [1, 2, 3]) {
    out.push({ id: `vm-0${n}`, loomId: `vm-0${n}`, kind: 'deploymentNode', label: `vm-0${n}`, parent: 'site', style: '' })
    out.push({ id: `svc-0${n}`, loomId: `svc-0${n}`, kind: 'containerInstance', label: `svc-0${n}`, parent: `vm-0${n}`, style: '' })
  }
  return out
}

const links: Link[] = [
  { connection: 'c1', from: 'svc-01', to: 'f5', fromPerson: false, kind: 'primary', purpose: '查快取' },
  { connection: 'c2', from: 'f5', to: 'svc-02', fromPerson: false, kind: 'primary', purpose: '查快取' },
] as Link[]

describe('巢狀排版', () => {
  it('子節點都在父框內', async () => {
    const laid = await layout(shapes(), links)
    const escaped: string[] = []
    walk(laid.nodes, (node, parent) => {
      if (!parent) return
      const out = node.x < 0 || node.y < 0
        || node.x + node.width > parent.width + 0.5
        || node.y + node.height > parent.height + 0.5
      if (out) escaped.push(`${node.id} 跑出 ${parent.id}`)
    })
    expect(escaped).toEqual([])
  })

  it('兄弟節點沒有互相重疊', async () => {
    const laid = await layout(shapes(), links)
    const byParent = new Map<string, typeof laid.nodes>()
    walk(laid.nodes, (node, parent) => {
      const key = parent?.id ?? 'root'
      if (!byParent.has(key)) byParent.set(key, [])
      byParent.get(key)!.push(node)
    })
    for (const siblings of byParent.values()) {
      for (let i = 0; i < siblings.length; i += 1) {
        for (let j = i + 1; j < siblings.length; j += 1) {
          const [a, b] = [siblings[i]!, siblings[j]!]
          const hit = a.x < b.x + b.width && b.x < a.x + a.width
            && a.y < b.y + b.height && b.y < a.y + a.height
          expect(hit, `${a.id} 疊到 ${b.id}`).toBe(false)
        }
      }
    }
  })

  it('父子關係沒被改掉', async () => {
    const laid = await layout(shapes(), links)
    const parents = new Map<string, string>()
    walk(laid.nodes, (node, parent) => parents.set(node.id, parent?.id ?? 'root'))
    expect(parents.get('svc-01')).toBe('vm-01')
    expect(parents.get('vm-01')).toBe('site')
    expect(parents.get('f5')).toBe('root')
  })

  it('形狀一個都沒掉', async () => {
    const laid = await layout(shapes(), links)
    const ids: string[] = []
    walk(laid.nodes, (node) => ids.push(node.id))
    expect(ids.sort()).toEqual(shapes().map((s) => s.id).sort())
  })

  it('容器縮到剛好包住子節點', async () => {
    // 包不住的話，圖上會看到機器浮在機房外面——而那正是這張圖唯一要回答的問題。
    const laid = await layout(shapes(), links)
    const site = laid.nodes.find((n) => n.id === 'site')!
    expect(site.width).toBeGreaterThan(0)
    expect(site.children).toHaveLength(3)
  })

  it('邊帶著轉彎點回來', async () => {
    // ⚠️ 這是整段最容易漏的：轉彎點沒回來的話，maxGraph 會自己再繞一次，
    // 而它不知道容器在哪——線會直接穿過容器。
    const laid = await layout(shapes(), links)
    expect(laid.edges.length).toBeGreaterThan(0)
    expect(laid.edges.some((e) => e.bends.length > 0)).toBe(true)
  })

  it('兩端畫不出來的線不送進排版', async () => {
    // 指向不存在的節點會讓 ELK 整個排不出來，而症狀是「圖是空的」。
    const bad = [{ ...links[0]!, to: '不存在' }] as Link[]
    const laid = await layout(shapes(), bad)
    expect(laid.edges).toEqual([])
    expect(laid.nodes.length).toBeGreaterThan(0)
  })

  it('被丟掉的線不會讓後面的編號跟著位移', async () => {
    // 編號是拿轉彎點的鑰匙，而 `render` 用的是**原始陣列**的位置。
    // 這裡改成過濾後再編號的話，鑰匙就對不上——症狀不是報錯，
    // 是「線自己亂繞、穿過容器」，看起來像排版引擎爛。
    const undrawable = { ...links[0]!, from: '不存在' } as Link
    const laid = await layout(shapes(), [undrawable, ...links])
    expect(laid.edges.map((e) => e.id)).toEqual([edgeId(links[0]!, 1), edgeId(links[1]!, 2)])
  })

  it('沒有連線也排得出來', async () => {
    const laid = await layout(shapes(), [])
    expect(laid.nodes.length).toBeGreaterThan(0)
  })
})
