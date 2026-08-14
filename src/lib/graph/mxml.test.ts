/**
 * `.drawio` 的讀寫。
 *
 * # 這裡真正在守的東西
 *
 * **檔案格式不能變。** 換掉畫布引擎是我們家的事，使用者的 `.drawio` 檔案
 * 不該因此打不開，`reconcile` 也不該因此看不見 `loomId`。
 *
 * maxGraph 自己的匯出**會**改格式（`<GraphDataModel>`／`<Cell>`／
 * `<Geometry _x>`），所以寫這一半是我們自己做的，而這份測試就是那條界線。
 */

import { describe, expect, it } from 'vitest'
import { GraphDataModel } from '@maxgraph/core'
import { readXml, writeXml } from './mxml'

/** 一份長得像我們產出來的圖：巢狀、帶 loomId、帶轉彎點。 */
const DRAWIO = `<mxfile host="diagram-loom">
  <diagram id="env-prod" name="prod">
    <mxGraphModel dx="0" dy="0" grid="1" gridSize="10" page="1" pageWidth="1169" pageHeight="827">
      <root>
        <mxCell id="0"/>
        <mxCell id="1" parent="0"/>
        <object label="vm-01&#10;虛擬機" loomId="n-vm" loomKind="deploymentNode" id="n-vm">
          <mxCell style="rounded=0;fillColor=none;verticalAlign=top;" vertex="1" parent="1">
            <mxGeometry x="40" y="80" width="180" height="60" as="geometry"/>
          </mxCell>
        </object>
        <object label="redis-01" loomId="i-redis" loomKind="containerInstance" id="i-redis">
          <mxCell style="rounded=1;" vertex="1" parent="n-vm">
            <mxGeometry x="12" y="24" width="160" height="30" as="geometry"/>
          </mxCell>
        </object>
        <object label="查快取" loomId="conn-1" loomKind="connection" id="conn-1:a:b">
          <mxCell style="edgeStyle=orthogonalEdgeStyle;" edge="1" parent="1" source="n-vm" target="i-redis">
            <mxGeometry relative="1" as="geometry">
              <Array as="points">
                <mxPoint x="206" y="110"/>
                <mxPoint x="206" y="202"/>
              </Array>
            </mxGeometry>
          </mxCell>
        </object>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>`

function load(xml = DRAWIO) {
  const model = new GraphDataModel()
  readXml(model, xml)
  return model
}

describe('讀 .drawio', () => {
  it('讀得到每一個形狀', () => {
    const model = load()
    for (const id of ['n-vm', 'i-redis', 'conn-1:a:b']) {
      expect(model.getCell(id), `${id} 沒讀到`).toBeTruthy()
    }
  })

  it('loomId 進得來——對帳靠的就是它', () => {
    const value = load().getCell('n-vm')?.getValue() as Element
    expect(value.getAttribute('loomId')).toBe('n-vm')
    expect(value.getAttribute('loomKind')).toBe('deploymentNode')
  })

  it('多行的標籤沒有被拆掉', () => {
    const value = load().getCell('n-vm')?.getValue() as Element
    expect(value.getAttribute('label')).toBe('vm-01\n虛擬機')
  })

  it('巢狀關係讀得出來', () => {
    // 攤平的話這張圖就看不出「東西跑在哪台機器上」。
    expect(load().getCell('i-redis')?.parent?.id).toBe('n-vm')
  })

  it('座標與轉彎點都在', () => {
    const model = load()
    const geo = model.getCell('n-vm')?.getGeometry()
    expect([geo?.x, geo?.y, geo?.width]).toEqual([40, 80, 180])
    expect(model.getCell('conn-1:a:b')?.getGeometry()?.points).toHaveLength(2)
  })

  it('沒有 mxfile 外殼也讀得進去', () => {
    // 使用者上傳的檔案兩種都有。少判斷這一下的代價是「打開是一片空白」。
    const bare = DRAWIO.replace(/<\/?mxfile[^>]*>|<\/?diagram[^>]*>/g, '')
    expect(load(bare).getCell('n-vm')).toBeTruthy()
  })
})

describe('寫回 .drawio', () => {
  const out = () => writeXml(load(), { id: 'env-prod', name: 'prod' })

  it('寫出來的是 draw.io 的格式，不是 maxGraph 的', () => {
    // maxGraph 自己的匯出會變成 <GraphDataModel>/<Cell>/<Geometry _x>，
    // 那種檔案 draw.io 打不開，reconcile 也讀不懂。
    const xml = out()
    expect(xml).toContain('<mxGraphModel')
    expect(xml).toContain('<mxCell')
    expect(xml).toContain('<mxGeometry')
    expect(xml).not.toContain('<GraphDataModel')
    expect(xml).not.toContain('_x=')
  })

  it('loomId 與 loomKind 寫得回去', () => {
    expect(out()).toContain('loomId="n-vm"')
    expect(out()).toContain('loomKind="deploymentNode"')
  })

  it('style 從物件變回字串', () => {
    // maxGraph 內部是物件，draw.io 的檔案格式是 `key=value;` 字串。
    expect(out()).toMatch(/style="[^"]*fillColor=none[^"]*"/)
  })

  it('轉彎點寫得回去', () => {
    // 掉了的話，使用者手工繞的線存一次檔就全部彈回去。
    const xml = out()
    expect(xml).toContain('<Array as="points">')
    expect(xml).toContain('<mxPoint x="206" y="110"/>')
  })

  it('巢狀關係寫得回去', () => {
    expect(out()).toMatch(/id="i-redis"[\s\S]{0,200}parent="n-vm"/)
  })

  it('讀進來再寫回去，形狀一個都沒少', () => {
    // 這是對帳的前提：存一次檔不會讓任何元素從圖上消失。
    const again = load(out())
    for (const id of ['n-vm', 'i-redis', 'conn-1:a:b']) {
      expect(again.getCell(id), `${id} 在來回一趟之後不見了`).toBeTruthy()
    }
    expect((again.getCell('n-vm')?.getValue() as Element).getAttribute('loomId')).toBe('n-vm')
    expect(again.getCell('i-redis')?.parent?.id).toBe('n-vm')
    expect(again.getCell('conn-1:a:b')?.getGeometry()?.points).toHaveLength(2)
  })

  it('座標四捨五入，不留浮點殘渣', () => {
    // `alignCells` 會留下 12.000000000000028 這種數字。這個專案的圖會存檔、
    // 會 diff、會對帳——每次存出不同的數字是最難查的那種雜訊。
    const model = load()
    const geo = model.getCell('n-vm')!.getGeometry()!.clone()
    geo.x = 12.000000000000028
    model.setGeometry(model.getCell('n-vm')!, geo)
    expect(writeXml(model, { id: 'e', name: 'p' })).toContain('x="12"')
  })

  it('名字裡的特殊字元會跳脫', () => {
    const model = load()
    const value = model.getCell('n-vm')!.getValue() as Element
    value.setAttribute('label', 'A & B <prod>')
    expect(writeXml(model, { id: 'e', name: 'p' })).toContain('A &amp; B &lt;prod&gt;')
  })
})
