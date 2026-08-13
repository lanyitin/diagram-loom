/**
 * 模型 → draw.io XML。
 *
 * # 這裡真正在守的東西
 *
 * **每一個代表模型元素的形狀都必須帶 `loomId`。** 沒有 `loomId` 的形狀
 * 對帳看不見——所以漏掉一個不會有人叫，圖上那個元素就從此跟模型脫鉤。
 * 那是個安靜的失敗，正好是這個工具最不能出的那種。
 *
 * 還有一條：**同一個 `loomId` 不能出現兩次**。對帳是集合比對
 * （`reconcile::DiagramElement` 只有 `{id, label}`），重複的話兩邊永遠對不起來。
 */

import { describe, expect, it } from 'vitest'
import { boundShapes, shapesOf, toXml } from './diagram'
import type { Environment, Project } from './model'

function project(): Project {
  return {
    id: 'p',
    slug: 'shop',
    name: '網路商店',
    logical: {
      people: [],
      systems: [
        { id: 's-shop', slug: 'shop', name: '商店', external: false, endpoints: [] },
        { id: 's-pay', slug: 'payment', name: '金流系統', external: true, endpoints: [] },
      ],
      containers: [{ id: 'c-redis', slug: 'redis', name: 'Redis 快取', system: 's-shop', endpoints: [] }],
      relationships: [],
    },
    environments: [],
  } as unknown as Project
}

function environment(): Environment {
  return {
    id: 'env-prod',
    slug: 'prod',
    name: '正式環境',
    nodes: [
      {
        id: 'n-site',
        slug: 'dc-main',
        kind: 'site',
        children: [
          {
            id: 'n-vm',
            slug: 'vm-redis-01',
            kind: 'virtual-machine',
            children: [],
            instances: [
              {
                id: 'i-redis',
                slug: 'redis-01',
                container: 'c-redis',
                endpoints: [{ id: 'ep', slug: 'client-port', protocol: 'tcp', address: '10.0.1.11:6379' }],
              },
            ],
          },
        ],
        instances: [],
      },
    ],
    infra: [
      { id: 'n-f5', slug: 'f5-01', endpoints: [{ id: 'vip', slug: 'vip', protocol: 'tcp', address: '10.0.0.100:6379' }] },
    ],
    systems: [{ id: 'si-pay', slug: 'payment-gw', system: 's-pay', endpoints: [] }],
    connections: [],
  } as unknown as Environment
}

describe('模型畫成圖', () => {
  it('每一種模型元素都畫得出來', () => {
    const shapes = shapesOf(project(), environment())
    expect(shapes.map((s) => s.kind).sort()).toEqual([
      'containerInstance',
      'deploymentNode',
      'deploymentNode',
      'infrastructureNode',
      'softwareSystemInstance',
    ])
  })

  it('每一個形狀都帶 loomId', () => {
    // 沒有 loomId 的形狀對帳看不見。漏掉一個不會有人叫，
    // 圖上那個元素就從此跟模型脫鉤——安靜的失敗。
    for (const s of shapesOf(project(), environment())) {
      expect(s.loomId, `${s.label} 沒有 loomId`).toBeTruthy()
    }
  })

  it('同一個 loomId 不會出現兩次', () => {
    // 對帳是集合比對，重複的話兩邊永遠對不起來。
    const ids = shapesOf(project(), environment()).map((s) => s.loomId)
    expect(new Set(ids).size).toBe(ids.length)
  })

  it('巢狀關係畫成巢狀', () => {
    // 站點 → 機器 → 落地。攤平的話這張圖就看不出「東西跑在哪裡」，
    // 而那正是 Deployment 圖唯一要回答的問題。
    const shapes = shapesOf(project(), environment())
    const byId = Object.fromEntries(shapes.map((s) => [s.loomId, s]))
    expect(byId['n-site']!.parent).toBeUndefined()
    expect(byId['n-vm']!.parent).toBe('n-site')
    expect(byId['i-redis']!.parent).toBe('n-vm')
  })

  it('落地上畫出位址', () => {
    // 這張圖最常被拿去做的事就是核對 IP。
    const shapes = shapesOf(project(), environment())
    expect(shapes.find((s) => s.loomId === 'i-redis')!.detail).toBe('10.0.1.11:6379')
  })

  it('外部系統畫成虛線，跟自家的分得出來', () => {
    const shapes = shapesOf(project(), environment())
    expect(shapes.find((s) => s.loomId === 'si-pay')!.style).toContain('dashed=1')
    expect(shapes.find((s) => s.loomId === 'i-redis')!.style).not.toContain('dashed=1')
  })
})

describe('XML', () => {
  it('是一份完整的 mxfile', () => {
    const xml = toXml(project(), environment())
    expect(xml).toContain('<mxfile')
    expect(xml).toContain('<mxGraphModel')
    // 這兩個 root cell 少了的話 draw.io 讀不進去。
    expect(xml).toContain('<mxCell id="0"/>')
    expect(xml).toContain('<mxCell id="1" parent="0"/>')
  })

  it('每個 loomId 都寫進 XML', () => {
    const xml = toXml(project(), environment())
    for (const id of ['n-site', 'n-vm', 'i-redis', 'n-f5', 'si-pay']) {
      expect(xml, `${id} 沒有寫進去`).toContain(`loomId="${id}"`)
    }
  })

  it('名字裡的特殊字元會跳脫', () => {
    // 少了這個，一個 `&` 就會讓整張圖讀不進去，
    // 而錯誤訊息只會說「XML 格式錯誤」。
    const p = project()
    const env = environment()
    env.nodes![0]!.slug = 'A & B <prod>'
    const xml = toXml(p, env)
    expect(xml).toContain('A &amp; B &lt;prod&gt;')
    expect(xml).not.toContain('A & B <prod>')
  })

  it('座標全部給 0，排版交給 draw.io', () => {
    // 自己算座標等於重寫一個排版引擎，而它內建的 ELK 已經夠好。
    //
    // 只看 mxGeometry。寫成 `not.toMatch(/x="[1-9]/)` 會抓到 `vertex="1"`——
    // 斷言鬆到抓錯東西的話，它守的就不是你以為的那件事了。
    const xml = toXml(project(), environment())
    const geometries = xml.match(/<mxGeometry[^>]*>/g) ?? []
    expect(geometries.length).toBeGreaterThan(0)
    for (const g of geometries) {
      expect(g, '座標不該是我們算的').toContain('x="0" y="0"')
    }
  })
})

describe('從圖上讀回 loomId', () => {
  it('挖得出來，而且只挖有綁定的', () => {
    // 沒有 loomId 的形狀是使用者自己畫的裝飾，對帳不該把它當成缺漏。
    const xml = `<mxfile><diagram><mxGraphModel><root>
      <mxCell id="0"/>
      <object label="redis-01&#10;10.0.1.11:6379" loomId="i-redis" id="a"><mxCell vertex="1"/></object>
      <mxCell id="note" value="使用者自己畫的便利貼" vertex="1"/>
    </root></mxGraphModel></diagram></mxfile>`
    expect(boundShapes(xml)).toEqual([{ id: 'i-redis', label: 'redis-01' }])
  })

  it('標籤只取第一行', () => {
    // 形狀上畫的是「名字＋位址」兩行，對帳只認名字。
    const xml = `<root><object label="vm-01&#10;虛擬機" loomId="n-1"/></root>`
    expect(boundShapes(xml)[0]!.label).toBe('vm-01')
  })

  it('產出去再讀回來，id 一個不少', () => {
    // 這是對帳的前提。階段 0 實測過 draw.io 不會弄丟這些屬性，
    // 這裡守的是我們自己這一半沒有寫壞。
    const xml = toXml(project(), environment())
    const back = boundShapes(xml).map((s) => s.id).sort()
    const want = shapesOf(project(), environment()).map((s) => s.loomId).sort()
    expect(back).toEqual(want)
  })
})
