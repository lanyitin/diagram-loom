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
import {
  contextDrawing,
  bind,
  blankXml,
  boundShapes,
  dim,
  canvasShapes,
  peopleOf,
  shapesOf,
  spotlight,
  toXml,
  unbind,
  unboundShapes,
  undim,
} from './diagram'
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
    systems: [
      {
        id: 'si-pay',
        slug: 'payment-gw',
        system: 's-pay',
        endpoints: [{ id: 'pay-ep', slug: 'api', protocol: 'tcp', address: '10.100.7.38:443' }],
      },
    ],
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
    // 人住邏輯層、沒有實體，不在 reconcile::model_elements 裡。
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
    // 站點 → 機器 → 服務實體。攤平的話這張圖就看不出「東西跑在哪裡」，
    // 而那正是 Deployment 圖唯一要回答的問題。
    const shapes = shapesOf(project(), environment())
    const byId = Object.fromEntries(shapes.map((s) => [s.loomId, s]))
    expect(byId['n-site']!.parent).toBeUndefined()
    expect(byId['n-vm']!.parent).toBe('n-site')
    expect(byId['i-redis']!.parent).toBe('n-vm')
  })

  it('服務實體上畫出位址', () => {
    // 這張圖最常被拿去做的事就是核對 IP。
    const shapes = shapesOf(project(), environment())
    expect(shapes.find((s) => s.loomId === 'i-redis')!.detail).toBe('10.0.1.11:6379')
  })

  it('外部系統實體上也畫出位址', () => {
    // 外部系統的位址是最常被拿去核對防火牆的東西。曾經這裡印的是
    // 系統名字，圖上只剩「金流系統」四個字，等於把 IP 藏起來。
    const shapes = shapesOf(project(), environment())
    expect(shapes.find((s) => s.loomId === 'si-pay')!.detail).toBe('10.100.7.38:443')
  })

  it('外部系統沒填位址時退回系統名字', () => {
    const env = environment()
    env.systems![0]!.endpoints = []
    const shapes = shapesOf(project(), env)
    expect(shapes.find((s) => s.loomId === 'si-pay')!.detail).toBe('金流系統')
  })

  it('外部系統畫成虛線，跟自家的分得出來', () => {
    const shapes = shapesOf(project(), environment())
    expect(shapes.find((s) => s.loomId === 'si-pay')!.style).toContain('dashed=1')
    expect(shapes.find((s) => s.loomId === 'i-redis')!.style).not.toContain('dashed=1')
  })
})

describe('畫布上該有的形狀', () => {
  const fromPerson = () => [link({ from: 'per-1', fromPerson: true, to: 'i-redis' })]

  it('起點是人的線，兩端在畫布上都找得到', () => {
    // 這條才是重點。人漏掉的症狀**不是**「圖上少一個人」——那還看得出來。
    // 症狀是那條線兩端對不上、被安靜地丟掉，而丟掉一條線會讓後面每一條
    // 線的轉彎點整批錯開（見 `graph/layout.ts` 的 `edgeId`），
    // 整張圖變成穿過所有容器的蜘蛛網。看起來像排版引擎爛。
    const ids = new Set(canvasShapes(project(), environment(), fromPerson()).map((s) => s.id))
    for (const l of fromPerson()) {
      expect(ids.has(l.from) && ids.has(l.to), `${l.from} → ${l.to} 有一端畫不出來`).toBe(true)
    }
  })

  it('沒有連線用到的人就不畫', () => {
    // 部署圖上冒出一堆跟這個環境無關的角色，比少畫更難讀。
    const shapes = canvasShapes(project(), environment(), [link({})])
    expect(shapes.some((s) => s.kind === 'person')).toBe(false)
  })

  it('人畫上去了，但仍然刻意沒有 loomId', () => {
    // 人不在 `reconcile::model_elements` 裡。給了 loomId，
    // 對帳就會說「圖上有這個、模型沒有」——一個假的缺漏。
    const person = canvasShapes(project(), environment(), fromPerson()).find((s) => s.kind === 'person')
    expect(person?.loomId).toBeUndefined()
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

describe('調暗', () => {
  function lit(ids: string[]) {
    return dim(toXml(project(), environment(), [link({})]), new Set(ids))
  }

  it('沒被點亮的形狀調暗', () => {
    const xml = lit(['i-redis'])
    expect(xml).toMatch(/loomId="n-site"[\s\S]*?opacity=25/)
  })

  it('點亮的形狀不帶 opacity', () => {
    // 設成 100 而不是拿掉的話，使用者自己調的半透明會被我們永久蓋掉。
    const xml = lit(['i-redis'])
    const cell = xml.match(/loomId="i-redis"[\s\S]{0,300}?<\/mxCell>/)![0]
    expect(cell).not.toContain('opacity=')
  })

  it('只動 opacity，其他樣式一個字都不改', () => {
    // style 字串裡住著使用者調的顏色與字體。這支螢光筆只有一個顏色。
    const xml = lit([])
    expect(xml).toContain('shape=hexagon')
    expect(xml).toContain('verticalAlign=top')
    expect(xml).toContain('dashed=1')
  })

  it('重複套用不會疊出兩個 opacity', () => {
    // 篩選會被按很多次。疊起來的話 style 字串會越長越髒，
    // 而 draw.io 只認最後一個——症狀是「調了但沒反應」。
    const once = lit([])
    const twice = dim(once, new Set())
    expect(twice.match(/opacity=/g)!.length).toBe(once.match(/opacity=/g)!.length)
  })

  it('調暗不會弄丟任何形狀', () => {
    // 這是整個設計的支點：篩選不改變圖上有什麼，所以對帳不受影響。
    const before = toXml(project(), environment(), [link({})])
    expect(boundShapes(dim(before, new Set())).map((s) => s.id).sort())
      .toEqual(boundShapes(before).map((s) => s.id).sort())
  })

  it('調暗不會弄丟座標', () => {
    // 從模型重產會洗掉手工排好的版面。這裡是改，不是重產。
    const moved = toXml(project(), environment(), []).replace('x="0" y="0"', 'x="640" y="280"')
    expect(dim(moved, new Set())).toContain('x="640"')
  })

  it('使用者自己畫的裝飾不碰', () => {
    // 沒有 loomKind 的形狀不是我們的東西。
    const note = '<root><mxCell id="n" value="便利貼" style="rounded=1;" vertex="1"/></root>'
    expect(dim(note, new Set())).not.toContain('opacity')
  })

  it('線也調得暗', () => {
    const xml = lit(['i-redis'])
    expect(xml).toMatch(/loomKind="connection"[\s\S]*?opacity=25/)
  })
})

/**
 * 標註：使用者上傳自己畫的圖，我們在上面補「這個框是誰」。
 *
 * 這裡守的是三件事：**沒指定的一個都不能漏掉**（漏掉的那個沒有人會發現）、
 * **指定過的收得回來**（指錯不能變成永久事實）、
 * **指定不會弄壞圖**（座標與使用者的樣式都要留著）。
 */
describe('標註沒指定的形狀', () => {
  /** 一張使用者自己畫的圖：兩個框、一條線，全部沒有 loomId。 */
  const theirs = `<mxfile><diagram><mxGraphModel><root>
    <mxCell id="0"/>
    <mxCell id="1" parent="0"/>
    <mxCell id="a" value="Apache 叢集" style="rounded=1;fillColor=#ffcc00;" vertex="1" parent="1">
      <mxGeometry x="120" y="80" width="180" height="60" as="geometry"/>
    </mxCell>
    <mxCell id="b" value="&lt;b&gt;F5&lt;/b&gt;&lt;br&gt;北區" style="shape=hexagon;" vertex="1" parent="1"/>
    <mxCell id="line" value="查快取" style="edgeStyle=orthogonalEdgeStyle;" edge="1" parent="1" source="a" target="b"/>
    <mxCell id="deco" style="ellipse;" vertex="1" parent="1"/>
  </root></mxGraphModel></diagram></mxfile>`

  it('使用者畫的形狀全部列得出來', () => {
    expect(unboundShapes(theirs).map((s) => s.cell)).toEqual(['a', 'b', 'line', 'deco'])
  })

  it('線標成線，框標成框', () => {
    // 線只能指給連線、框只能指給元素（Rust 那邊分兩份清單）。
    // 分不出來的話，一個方框可以被指成一條連線，而對帳會永遠對不上它。
    const byCell = Object.fromEntries(unboundShapes(theirs).map((s) => [s.cell, s.edge]))
    expect(byCell).toEqual({ a: false, b: false, line: true, deco: false })
  })

  it('標籤是 HTML 也讀得出字', () => {
    // 使用者的圖上常常是 <b>F5</b><br>北區。整段拿去比對名字會一個都對不上。
    expect(unboundShapes(theirs).find((s) => s.cell === 'b')!.label).toBe('F5')
  })

  it('沒有文字的形狀照樣列出來', () => {
    // 空白方框多半是裝飾，但自己決定「這個不用管」，
    // 使用者就永遠不知道有這回事——這個工具的命是怕漏。
    expect(unboundShapes(theirs).find((s) => s.cell === 'deco')!.label).toBe('')
  })

  it('root 的那兩個 cell 不算形狀', () => {
    expect(unboundShapes(theirs).map((s) => s.cell)).not.toContain('0')
  })

  it('我們自己畫的圖，一個都不會被當成沒指定', () => {
    // 反過來說：這裡若有漏網之魚，使用者會被叫去指定一個他根本沒畫的東西。
    expect(unboundShapes(toXml(project(), environment(), [link({})]))).toEqual([])
  })

  it('人不算沒指定——它本來就不該有 loomId', () => {
    const xml = toXml(project(), environment(), [link({ from: 'per-1', fromPerson: true })])
    expect(unboundShapes(xml).map((s) => s.cell)).not.toContain('per-1')
  })
})

describe('指定一個形狀代表誰', () => {
  const box = `<root><mxCell id="a" value="Apache 叢集" style="rounded=1;fillColor=#ffcc00;" vertex="1" parent="1">
    <mxGeometry x="120" y="80" width="180" height="60" as="geometry"/>
  </mxCell></root>`

  it('指定之後對帳看得見它了', () => {
    // 這就是整件事的目的：使用者自己畫的框，從此跟模型接上。
    expect(boundShapes(bind(box, 'a', 'i-apache'))).toEqual([
      { id: 'i-apache', label: 'Apache 叢集' },
    ])
  })

  it('指定之後就不再是「沒指定」', () => {
    expect(unboundShapes(bind(box, 'a', 'i-apache'))).toEqual([])
  })

  it('座標與使用者的樣式原封不動', () => {
    // 這是他自己排的版、自己挑的顏色。指定只是貼一張標籤上去。
    const after = bind(box, 'a', 'i-apache')
    expect(after).toContain('x="120"')
    expect(after).toContain('fillColor=#ffcc00')
  })

  it('包起來之後 id 只留一個', () => {
    // draw.io 認外層那個。裡面再留一個，遲早會對不上。
    const after = bind(box, 'a', 'i-apache')
    expect(after.match(/id="a"/g)).toHaveLength(1)
  })

  it('本來就有 object 包著的，直接加屬性', () => {
    // 使用者自己用「編輯資料」加過欄位的形狀。不能把他的東西拆掉。
    const withData = `<root><object label="F5" id="b" tooltip="北區"><mxCell vertex="1"/></object></root>`
    const after = bind(withData, 'b', 'n-f5')
    expect(after).toContain('tooltip="北區"')
    expect(boundShapes(after)).toEqual([{ id: 'n-f5', label: 'F5' }])
  })

  it('找不到那個 cell 就原樣退回', () => {
    expect(bind(box, '不存在', 'i-apache')).toBe(box)
  })

  it('取消指定會回到原本的樣子', () => {
    // 指錯了一定要收得回來：綁定之後對帳就會把它當事實。
    const back = unbind(bind(box, 'a', 'i-apache'), 'a')
    expect(unboundShapes(back).map((s) => s.label)).toEqual(['Apache 叢集'])
    expect(boundShapes(back)).toEqual([])
    expect(back).toContain('x="120"')
  })

  it('取消指定不會拆掉使用者自己加的資料', () => {
    const withData = `<root><object label="F5" id="b" tooltip="北區"><mxCell vertex="1"/></object></root>`
    const back = unbind(bind(withData, 'b', 'n-f5'), 'b')
    expect(back).toContain('tooltip="北區"')
    expect(back).not.toContain('loomId')
  })
})

describe('標示是圖上哪一個', () => {
  const two = `<root>
    <mxCell id="a" value="一" style="rounded=1;" vertex="1" parent="1"/>
    <mxCell id="b" value="二" style="rounded=1;" vertex="1" parent="1"/>
  </root>`

  it('只有選中的那個亮著', () => {
    // 清單上 47 個名字，光靠名字對不出來——而對不出來的人就會亂指。
    const xml = spotlight(two, 'a')
    expect(xml).toMatch(/id="a"[^>]*style="rounded=1;"/)
    expect(xml).toMatch(/id="b"[^>]*opacity=25/)
  })

  it('擦得掉，圖會回到全部亮著', () => {
    // 少了這個，關掉標註之後圖還是暗的——opacity 已經寫進 XML 了。
    expect(undim(spotlight(two, 'a'))).not.toContain('opacity')
  })

  it('關掉篩選也擦得乾淨', () => {
    const filtered = dim(toXml(project(), environment(), [link({})]), new Set())
    expect(undim(filtered)).not.toContain('opacity=25')
  })

  it('標示不會弄丟形狀', () => {
    // 跟篩選同一條規矩：調暗不是隱藏，圖上的東西一個都沒少。
    expect(unboundShapes(spotlight(two, 'a')).map((s) => s.cell)).toEqual(['a', 'b'])
  })
})

describe('空白的新圖', () => {
  it('draw.io 讀得進去', () => {
    // 那兩個 root cell 少了會變成一片空白——跟「這張圖本來就是空的」
    // 長得一模一樣，看不出是壞掉了。
    const xml = blankXml('env-prod', '我的簡報圖')
    expect(xml).toContain('<mxCell id="0"/>')
    expect(xml).toContain('<mxCell id="1" parent="0"/>')
    expect(xml).toContain('name="我的簡報圖"')
  })

  it('上面一個形狀都沒有', () => {
    expect(unboundShapes(blankXml('e', '空的'))).toEqual([])
    expect(boundShapes(blankXml('e', '空的'))).toEqual([])
  })

  it('名字裡的特殊字元會跳脫', () => {
    expect(blankXml('e', 'A & B')).toContain('A &amp; B')
  })
})


describe('Context 圖', () => {
  /**
   * 一份剛好踩到每一種情況的模型：自家系統、外部系統、沒部署到這個環境的
   * 系統、同一個系統內部的契約，以及同一對系統之間的兩條契約。
   */
  function shop(): Project {
    return {
      id: 'p',
      slug: 'shop',
      name: '網路商店',
      logical: {
        people: [
          { id: 'per-1', slug: '客戶', name: '客戶' },
          { id: 'per-2', slug: '客服', name: '客服' },
        ],
        systems: [
          { id: 's-shop', slug: 'shop', name: '商店', external: false, endpoints: [] },
          { id: 's-pay', slug: 'payment', name: '金流系統', external: true, endpoints: [] },
          { id: 's-old', slug: 'legacy', name: '舊主機', external: false, endpoints: [] },
        ],
        containers: [
          { id: 'c-api', slug: 'api', name: '訂單 API', system: 's-shop', endpoints: [] },
          { id: 'c-redis', slug: 'redis', name: 'Redis 快取', system: 's-shop', endpoints: [] },
          { id: 'c-old', slug: 'old-api', name: '舊 API', system: 's-old', endpoints: [] },
        ],
        relationships: [
          { id: 'r-order', slug: 'order', purpose: '下單', from: { person: 'per-1' }, to: { container: 'c-api' }, to_endpoint: 'e' },
          { id: 'r-pay', slug: 'pay', purpose: '刷卡', from: { container: 'c-api' }, to: { system: 's-pay' }, to_endpoint: 'e' },
          { id: 'r-refund', slug: 'refund', purpose: '退款', from: { container: 'c-api' }, to: { system: 's-pay' }, to_endpoint: 'e' },
          { id: 'r-cache', slug: 'cache', purpose: '查快取', from: { container: 'c-api' }, to: { container: 'c-redis' }, to_endpoint: 'e' },
          { id: 'r-old', slug: 'old', purpose: '查舊資料', from: { container: 'c-api' }, to: { container: 'c-old' }, to_endpoint: 'e' },
        ],
      },
      environments: [],
    } as unknown as Project
  }

  function env(): Environment {
    return {
      id: 'env-prod',
      slug: 'prod',
      name: '正式環境',
      nodes: [
        {
          id: 'n-vm',
          slug: 'vm-01',
          kind: 'virtual-machine',
          children: [],
          instances: [
            { id: 'i-api', slug: 'api-01', container: 'c-api', endpoints: [] },
            { id: 'i-redis', slug: 'redis-01', container: 'c-redis', endpoints: [] },
          ],
        },
      ],
      infra: [],
      systems: [{ id: 'si-pay', slug: 'payment-gw', system: 's-pay', endpoints: [] }],
      connections: [],
    } as unknown as Environment
  }

  it('一個系統一個框，沒有機器也沒有服務', () => {
    const { shapes } = contextDrawing(shop(), env())
    expect(shapes.map((s) => s.kind).sort()).toEqual(['person', 'softwareSystem', 'softwareSystem'])
  })

  it('沒有部署到這個環境的系統不畫', () => {
    // 三個環境的 context 圖長得不一樣本身就是資訊。
    const { shapes } = contextDrawing(shop(), env())
    expect(shapes.map((s) => s.label)).not.toContain('legacy')
  })

  it('外部系統畫成虛線，跟自家的分得出來', () => {
    const { shapes } = contextDrawing(shop(), env())
    expect(shapes.find((s) => s.loomId === 's-pay')!.style).toContain('dashed=1')
    expect(shapes.find((s) => s.loomId === 's-shop')!.style).not.toContain('dashed=1')
  })

  it('每一個框都帶 loomId', () => {
    // 這張圖是簡圖，比對的模型側含邏輯層（reconcile::elements_for）。
    // 不給 loomId 的話，「有人把這個系統刪了」就變成沒有人會發現的事。
    for (const s of contextDrawing(shop(), env()).shapes) {
      expect(s.loomId, `${s.label} 沒有 loomId`).toBeTruthy()
    }
  })

  it('同一對系統之間的契約收成一條線', () => {
    const { links } = contextDrawing(shop(), env())
    const pay = links.filter((l) => l.from === 's-shop' && l.to === 's-pay')
    expect(pay).toHaveLength(1)
    expect(pay[0]!.purpose).toBe('2 條契約')
  })

  it('收攏過的線刻意沒有 loomId', () => {
    // 一條線代表一群契約。硬指其中一條等於說謊，而對帳會把綁定當事實。
    const { links } = contextDrawing(shop(), env())
    expect(links.find((l) => l.to === 's-pay')!.connection).toBe('')
  })

  it('剛好一條契約的線綁得回那一條，字也是它的用途', () => {
    const { links } = contextDrawing(shop(), env())
    const order = links.find((l) => l.from === 'per-1')!
    expect(order.connection).toBe('r-order')
    expect(order.purpose).toBe('下單')
    expect(order.fromPerson).toBe(true)
  })

  it('同一個系統裡的契約不畫', () => {
    // c-api 連 c-redis 兩邊都屬於 shop，畫出來是一個讀不出東西的圈。
    const { links } = contextDrawing(shop(), env())
    expect(links.some((l) => l.from === l.to)).toBe(false)
  })

  it('一端沒部署到這個環境的契約不畫', () => {
    const { links } = contextDrawing(shop(), env())
    expect(links.some((l) => l.to === 's-old')).toBe(false)
  })

  it('沒有線的人不畫', () => {
    // 客服在這個環境一條契約都沒有，畫上去只是一個沒有人解釋得了的框。
    const { shapes } = contextDrawing(shop(), env())
    expect(shapes.some((s) => s.loomId === 'per-2')).toBe(false)
  })

  it('副標是系統的全名，不是位址', () => {
    // 這張圖是給不熟這套系統的人看的，`payment` 對他沒有意義。
    const { shapes } = contextDrawing(shop(), env())
    expect(shapes.find((s) => s.loomId === 's-pay')!.detail).toBe('金流系統')
  })
})
