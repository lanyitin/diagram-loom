/**
 * 綁定與螢光筆——換了 substrate，規則不能換。
 *
 * 這兩件事以前是操作 XML 字串（`diagram.ts`），現在操作 maxGraph 的 cells。
 * 這份測試守的是**規則沒有跟著 substrate 一起變**：
 *
 * - 沒有 `loomId` 的形狀對帳看不見
 * - 一條萬用字元連線的 N×M 條線只算一次
 * - 調暗不刪任何東西
 * - 綁定寫在形狀上，不是記在別的地方
 */

import { beforeEach, describe, expect, it } from 'vitest'
import { GraphDataModel } from '@maxgraph/core'
import { attribute, bind, boundShapes, unboundShapes } from './binding'
import { dim, opacityOf, smeared, spotlight, undim } from './highlight'
import { readXml } from './mxml'

/** 我們畫的兩個形狀＋一條線，加上使用者自己畫的一個框與一張便利貼。 */
const XML = `<mxGraphModel><root>
  <mxCell id="0"/>
  <mxCell id="1" parent="0"/>
  <object label="vm-01&#10;虛擬機" loomId="n-vm" loomKind="deploymentNode" id="n-vm">
    <mxCell style="rounded=0;" vertex="1" parent="1"><mxGeometry x="0" y="0" width="180" height="60" as="geometry"/></mxCell>
  </object>
  <object label="客戶" loomKind="person" id="per-1">
    <mxCell style="shape=umlActor;" vertex="1" parent="1"><mxGeometry x="0" y="0" width="40" height="70" as="geometry"/></mxCell>
  </object>
  <object label="查快取" loomId="conn-1" loomKind="connection" id="conn-1:a:b">
    <mxCell style="edgeStyle=orthogonalEdgeStyle;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"/></mxCell>
  </object>
  <object label="查快取" loomId="conn-1" loomKind="connection" id="conn-1:a:c">
    <mxCell style="edgeStyle=orthogonalEdgeStyle;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"/></mxCell>
  </object>
  <mxCell id="note" value="&lt;b&gt;Apache&lt;/b&gt;&lt;br&gt;叢集" style="rounded=1;" vertex="1" parent="1">
    <mxGeometry x="0" y="0" width="120" height="60" as="geometry"/>
  </mxCell>
  <mxCell id="deco" style="ellipse;" vertex="1" parent="1">
    <mxGeometry x="0" y="0" width="40" height="40" as="geometry"/>
  </mxCell>
</root></mxGraphModel>`

let model: GraphDataModel

beforeEach(() => {
  model = new GraphDataModel()
  readXml(model, XML)
})

describe('圖上綁著什麼', () => {
  it('挖得出來，而且只挖有綁定的', () => {
    // 沒有 loomId 的形狀是使用者自己畫的裝飾，對帳不該把它當成缺漏。
    expect(boundShapes(model).map((s) => s.id).sort()).toEqual(['conn-1', 'n-vm'])
  })

  it('同一條連線的 N×M 條線只算一次', () => {
    // 模型側每條連線只有一個元素。不去重的話對帳的衝突判斷會錯。
    expect(boundShapes(model).filter((s) => s.id === 'conn-1')).toHaveLength(1)
  })

  it('標籤只取第一行', () => {
    // 形狀上畫的是「名字＋位址」兩行，對帳只認名字。
    expect(boundShapes(model).find((s) => s.id === 'n-vm')?.label).toBe('vm-01')
  })
})

describe('圖上還沒指定什麼', () => {
  it('使用者自己畫的才算', () => {
    expect(unboundShapes(model).map((s) => s.cell).sort()).toEqual(['deco', 'note'])
  })

  it('人不算——它本來就不該有 loomId', () => {
    expect(unboundShapes(model).map((s) => s.cell)).not.toContain('per-1')
  })

  it('標籤是 HTML 也讀得出字', () => {
    expect(unboundShapes(model).find((s) => s.cell === 'note')?.label).toBe('Apache')
  })

  it('沒有文字的形狀照樣列出來', () => {
    // 自己決定「這個不用管」，使用者就永遠不知道有這回事。
    expect(unboundShapes(model).find((s) => s.cell === 'deco')?.label).toBe('')
  })

  it('線標成線，框標成框', () => {
    const note = unboundShapes(model).find((s) => s.cell === 'note')
    expect(note?.edge).toBe(false)
  })
})

describe('指定與取消', () => {
  it('指定之後對帳看得見它了', () => {
    bind(model, 'note', 'i-apache')
    expect(boundShapes(model).map((s) => s.id)).toContain('i-apache')
    expect(unboundShapes(model).map((s) => s.cell)).not.toContain('note')
  })

  it('指定不會弄掉原本的文字', () => {
    bind(model, 'note', 'i-apache')
    expect(attribute(model.getCell('note')!, 'label')).toContain('Apache')
  })

  it('取消指定收得回來', () => {
    // 指錯了不能變成永久事實——對帳會把綁定當事實。
    bind(model, 'note', 'i-apache')
    bind(model, 'note', null)
    expect(unboundShapes(model).map((s) => s.cell)).toContain('note')
    expect(boundShapes(model).map((s) => s.id)).not.toContain('i-apache')
  })

  it('找不到那個 cell 就說沒做到', () => {
    expect(bind(model, '不存在', 'x')).toBe(false)
  })
})

describe('螢光筆', () => {
  it('沒被點亮的調暗', () => {
    dim(model, new Set(['n-vm']))
    expect(opacityOf(model.getCell('conn-1:a:b')!)).toBe(25)
  })

  it('點亮的不帶 opacity', () => {
    // 設成 100 而不是拿掉的話，使用者自己調的半透明會被永久蓋掉。
    dim(model, new Set(['n-vm']))
    expect(opacityOf(model.getCell('n-vm')!)).toBeUndefined()
  })

  it('只動 opacity，其他樣式一個字都不改', () => {
    dim(model, new Set())
    const style = model.getCell('n-vm')!.getStyle() as Record<string, unknown>
    expect(style.rounded).toBe(0)
  })

  it('篩選不碰使用者自己畫的裝飾', () => {
    dim(model, new Set())
    expect(opacityOf(model.getCell('note')!)).toBeUndefined()
  })

  it('標示會把使用者自己畫的也調暗', () => {
    // 要指的正是那些形狀，不碰它們就等於整張圖都亮著。
    spotlight(model, 'note')
    expect(opacityOf(model.getCell('note')!)).toBeUndefined()
    expect(opacityOf(model.getCell('deco')!)).toBe(25)
  })

  it('擦得掉', () => {
    spotlight(model, 'note')
    undim(model)
    expect(smeared(model)).toBe(false)
  })

  it('調暗不會弄丟任何形狀', () => {
    // 這是整個設計的支點：篩選不改變圖上有什麼，所以對帳不受影響。
    const before = boundShapes(model).map((s) => s.id).sort()
    dim(model, new Set())
    expect(boundShapes(model).map((s) => s.id).sort()).toEqual(before)
  })

  it('重複塗不會疊出兩個 opacity', () => {
    dim(model, new Set())
    dim(model, new Set())
    expect(opacityOf(model.getCell('n-vm')!)).toBe(25)
  })
})
