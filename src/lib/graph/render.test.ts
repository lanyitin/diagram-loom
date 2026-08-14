/**
 * 模型 → maxGraph 的 cells。
 *
 * # 這裡真正在守的東西
 *
 * 跟 draw.io 那時候同一條：**每一個代表模型元素的形狀都必須帶 `loomId`**。
 * 沒有的話對帳看不見——漏掉一個不會有人叫，圖上那個元素就從此跟模型脫鉤。
 *
 * 還有一條：**同一個 `loomId` 不能出現在兩個形狀上**（線是例外，
 * 一條萬用字元連線是 N×M 條線共用一個 id）。
 */

import { describe, expect, it } from 'vitest'
import { GraphDataModel } from '@maxgraph/core'
import { layout } from './layout'
import { render } from './render'
import { writeXml } from './mxml'
import type { Shape } from '../diagram'
import type { Link } from '../model'
import { STYLE } from '../diagram'

function shapes(): Shape[] {
  return [
    { id: 'site', loomId: 'site', kind: 'deploymentNode', label: 'dc', style: STYLE.site },
    { id: 'vm', loomId: 'vm', kind: 'deploymentNode', label: 'vm-01', parent: 'site', style: STYLE.node },
    { id: 'svc', loomId: 'svc', kind: 'containerInstance', label: 'redis-01', detail: '10.0.1.11:6379', parent: 'vm', style: STYLE.instance },
    { id: 'f5', loomId: 'f5', kind: 'infrastructureNode', label: 'f5-01', style: STYLE.infra },
    // 人：**刻意沒有 loomId**，它不在 reconcile::model_elements 裡。
    { id: 'per', kind: 'person', label: '客戶', style: STYLE.person },
  ]
}

const links: Link[] = [
  { connection: 'c1', from: 'svc', to: 'f5', fromPerson: false, kind: 'primary', purpose: '查快取' },
  { connection: 'c1', from: 'svc', to: 'f5', fromPerson: false, kind: 'primary', purpose: '查快取' },
  { connection: 'c2', from: 'f5', to: 'svc', fromPerson: false, kind: 'fallback', purpose: '備援' },
] as Link[]

async function built(useLinks = links) {
  const model = new GraphDataModel()
  const list = shapes()
  const laid = await layout(list, useLinks)
  const cells = render(model, list, useLinks, laid)
  return { model, cells }
}

const attr = (cell: unknown, name: string) =>
  ((cell as { getValue(): Element }).getValue()).getAttribute(name)

describe('畫進資料模型', () => {
  it('每一種模型元素都畫得出來', async () => {
    const { cells } = await built()
    for (const id of ['site', 'vm', 'svc', 'f5', 'per']) {
      expect(cells.get(id), `${id} 沒畫出來`).toBeTruthy()
    }
  })

  it('每一個模型元素都帶 loomId', async () => {
    const { cells } = await built()
    for (const id of ['site', 'vm', 'svc', 'f5']) {
      expect(attr(cells.get(id), 'loomId'), `${id} 沒有 loomId`).toBe(id)
    }
  })

  it('人刻意沒有 loomId', async () => {
    // 人住邏輯層、沒有實體。給了 loomId 對帳就會說「圖上有這個、模型沒有」。
    const { cells } = await built()
    expect(attr(cells.get('per'), 'loomId')).toBeNull()
    expect(attr(cells.get('per'), 'loomKind')).toBe('person')
  })

  it('巢狀關係畫成巢狀', async () => {
    // 攤平的話就看不出「東西跑在哪台機器上」，而那是部署圖唯一要回答的問題。
    const { cells } = await built()
    expect(cells.get('svc')?.parent?.id).toBe('vm')
    expect(cells.get('vm')?.parent?.id).toBe('site')
  })

  it('服務實體上畫出位址', async () => {
    const { cells } = await built()
    expect(attr(cells.get('svc'), 'label')).toBe('redis-01\n10.0.1.11:6379')
  })

  it('萬用字元的 N×M 全部畫出來，共用同一個 loomId', async () => {
    // 畫一條會讓人以為只有一對在通。lint 的可達性 BFS 就是這樣建圖的。
    const { model } = await built()
    const edges = model.getRoot()!.children![0]!.children!.filter((c) => c.isEdge())
    expect(edges.filter((e) => attr(e, 'loomId') === 'c1')).toHaveLength(2)
  })

  it('備援線看得出來跟平常的不一樣', async () => {
    const { model } = await built()
    const edges = model.getRoot()!.children![0]!.children!.filter((c) => c.isEdge())
    const fallback = edges.find((e) => attr(e, 'loomId') === 'c2')
    expect((fallback?.getStyle() as Record<string, unknown>).dashed).toBe(1)
  })

  it('兩端畫不出來的線就不畫', async () => {
    const bad = [{ ...links[0]!, to: '不存在' }] as Link[]
    const { model } = await built(bad)
    expect(model.getRoot()!.children![0]!.children!.filter((c) => c.isEdge())).toHaveLength(0)
  })

  it('有轉彎點的線不再給 edgeStyle', async () => {
    // 兩個東西同時決定線怎麼走，贏的是不知道容器在哪的那個——
    // 實測線會直接穿過容器。
    const { model } = await built()
    const edges = model.getRoot()!.children![0]!.children!.filter((c) => c.isEdge())
    for (const edge of edges) {
      const style = edge.getStyle() as Record<string, unknown>
      const bends = edge.getGeometry()?.points ?? []
      if (bends.length) expect(style.edgeStyle, '有轉彎點卻還掛著繞線器').toBeUndefined()
    }
  })
})

describe('畫完之後存得出 .drawio', () => {
  it('存出來的檔案帶得回每一個 loomId', async () => {
    // 這是整條路的終點：畫出來的東西存進檔案，對帳讀得到。
    const { model } = await built()
    const xml = writeXml(model, { id: 'env', name: 'prod' })
    for (const id of ['site', 'vm', 'svc', 'f5', 'c1']) {
      expect(xml, `${id} 沒有寫進檔案`).toContain(`loomId="${id}"`)
    }
    expect(xml).not.toContain('loomId="per"')
  })

  it('存出來的是 draw.io 的格式', async () => {
    const { model } = await built()
    const xml = writeXml(model, { id: 'env', name: 'prod' })
    expect(xml).toContain('<mxGraphModel')
    expect(xml).toContain('<mxCell id="0"/>')
    expect(xml).not.toContain('<GraphDataModel')
  })
})
