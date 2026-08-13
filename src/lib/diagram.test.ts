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
import { boundShapes, peopleOf, shapesOf, toXml } from './diagram'
import type { Environment, Link, Project } from './model'

function project(): Project {
  return {
    id: 'p',
    slug: 'shop',
    name: '網路商店',
    logical: {
      people: [{ id: 'per-1', slug: '客戶', name: '客戶' }],
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

function link(over: Partial<Link>): Link {
  return {
    connection: 'conn-1',
    from: 'i-redis',
    to: 'n-f5',
    fromPerson: false,
    kind: 'primary',
    purpose: '查快取',
    ...over,
  } as Link
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

  it('人刻意沒有 loomId', () => {
    // 人住邏輯層、沒有落地，不在 reconcile::model_elements 裡。
    // 給了 loomId 對帳就會說「圖上有這個、模型沒有」——一個假的缺漏。
    const [actor] = peopleOf(project(), [link({ from: 'per-1', fromPerson: true })])
    expect(actor!.id).toBe('per-1')
    expect(actor!.loomId).toBeUndefined()
  })

  it('沒被連線用到的人不畫', () => {
    // 部署圖上不該冒出一堆跟這個環境無關的角色。
    expect(peopleOf(project(), [])).toEqual([])
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

describe('線', () => {
  it('一條連線畫成一條線', () => {
    const xml = toXml(project(), environment(), [link({})])
    expect(xml).toContain('edge="1"')
    expect(xml).toContain('loomKind="connection"')
  })

  it('萬用字元的 N×M 全部畫出來，共用同一個 loomId', () => {
    // 3 台連 3 台是 9 條。lint 的可達性 BFS 就是這樣建圖的——
    // 畫 1 條會讓人以為只有一對在通。
    const links = [
      link({ from: 'i-redis', to: 'n-f5' }),
      link({ from: 'i-redis', to: 'si-pay' }),
    ]
    const xml = toXml(project(), environment(), links)
    expect(xml.match(/edge="1"/g)).toHaveLength(2)
    expect(xml.match(/loomId="conn-1"/g)).toHaveLength(2)
  })

  it('同一條連線的每條線，XML id 都不一樣', () => {
    // 重複的 id 會讓 draw.io 只留一條，圖上就看起來少畫了。
    const links = [
      link({ from: 'i-redis', to: 'n-f5' }),
      link({ from: 'i-redis', to: 'si-pay' }),
    ]
    const xml = toXml(project(), environment(), links)
    const ids = [...xml.matchAll(/id="(conn-1:[^"]+)"/g)].map((m) => m[1])
    expect(new Set(ids).size).toBe(ids.length)
  })

  it('兩端畫不出來的線就不畫', () => {
    // 指向不存在的形狀的線會變成一條飄在畫布上的浮線，
    // 看起來像連到別的地方——比不畫更糟。
    const xml = toXml(project(), environment(), [link({ to: '不存在' })])
    expect(xml).not.toContain('edge="1"')
  })

  it('備援線看得出來跟平常的不一樣', () => {
    // 一樣要建、防火牆一樣要開，但畫成一樣重的話讀不出主路徑。
    const xml = toXml(project(), environment(), [link({ kind: 'fallback' })])
    expect(xml).toMatch(/style="[^"]*dashed=1[^"]*"[^>]*edge="1"/)
  })

  it('人被連線用到時，圖上長出一個角色', () => {
    const xml = toXml(project(), environment(), [
      link({ from: 'per-1', fromPerson: true }),
    ])
    expect(xml).toContain('umlActor')
    // 但它不帶 loomId，對帳看不見。
    expect(xml).not.toContain('loomId="per-1"')
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
  it('同一條連線的 N×M 條線只算一次', () => {
    // 模型側每條連線只有一個元素。不去重的話同一條會被數很多次，
    // 對帳的衝突判斷跟著錯。
    const links = [
      link({ from: 'i-redis', to: 'n-f5' }),
      link({ from: 'i-redis', to: 'si-pay' }),
    ]
    const xml = toXml(project(), environment(), links)
    expect(boundShapes(xml).filter((s) => s.id === 'conn-1')).toHaveLength(1)
  })

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
