/**
 * 在圖上找形狀。
 *
 * 圖上的搜尋跟表格的搜尋是**兩件不同的事**：表格會少幾列，圖不能少東西
 * （藏掉幾個框，剩下的線就指向空氣）。所以這裡驗的是「帶你過去」該有的行為：
 * 找得到哪些、順序穩不穩、走到底會不會繞回來。
 */

import { describe, expect, it } from 'vitest'
import { GraphDataModel } from '@maxgraph/core'
import { matching, step } from './find'
import { centered } from './canvas'
import { shapeLabels } from './binding'
import { readXml } from './mxml'
import type { UnboundShape } from '../model'

const shape = (cell: string, label: string, edge = false): UnboundShape => ({ cell, label, edge })

const SHAPES = [
  shape('a', 'redis-01'),
  shape('b', 'redis-02'),
  shape('c', 'vm-APACHE-01'),
  shape('d', ''),
  shape('e', '查快取', true),
]

describe('比對', () => {
  it('比對得到部分字串', () => {
    expect(matching(SHAPES, 'redis').map((s) => s.cell)).toEqual(['a', 'b'])
  })

  it('不分大小寫', () => {
    // 表格那邊也是這樣（`rows.ts` 的 filterRows）。同一個工具裡兩種搜尋語意，
    // 使用者只會覺得「有時候找得到有時候找不到」。
    expect(matching(SHAPES, 'apache').map((s) => s.cell)).toEqual(['c'])
  })

  it('前後空白不算', () => {
    expect(matching(SHAPES, '  redis-01  ').map((s) => s.cell)).toEqual(['a'])
  })

  it('線也找得到', () => {
    // 線也是圖上的東西，而「那條連線在哪」是真的會問的問題。
    expect(matching(SHAPES, '查快取').map((s) => s.cell)).toEqual(['e'])
  })

  it('空字串回空的，不是全部', () => {
    // 沒打字時不該有一個「目前這一個」——那會在圖上留一個沒人解釋得了的亮框。
    expect(matching(SHAPES, '')).toEqual([])
    expect(matching(SHAPES, '   ')).toEqual([])
  })

  it('刻意不支援萬用字元', () => {
    // `redis-*` 那一套是拿來**指涉一群東西**的（連線的兩端），不是搜尋。
    // 兩種語意混在同一個輸入框裡，使用者無從得知自己在用哪一種。
    expect(matching(SHAPES, 'redis-*')).toEqual([])
  })

  it('照圖上的順序，每次都一樣', () => {
    // 順序會變的話，按兩次「下一個」可能跳回原地。
    expect(matching(SHAPES, 'redis').map((s) => s.cell)).toEqual(
      matching(SHAPES, 'redis').map((s) => s.cell),
    )
  })
})

describe('走到下一個', () => {
  it('走到底會繞回來', () => {
    // 走到底沒反應的話，使用者會以為卡住了。
    expect(step(2, 3, 1)).toBe(0)
    expect(step(0, 3, -1)).toBe(2)
  })

  it('一個都沒有時回 0，不會炸', () => {
    expect(step(5, 0, 1)).toBe(0)
  })

  it('把超出範圍的位置拉回來', () => {
    // 搜尋字改了、符合的變少了，舊的位置可能已經超出範圍。
    expect(step(9, 3, 0)).toBe(0)
  })
})

describe('從真的圖上取標籤', () => {
  /** 我們畫的兩個形狀＋一條線，加上使用者自己畫的一個框。 */
  const XML = `<mxGraphModel><root>
    <mxCell id="0"/>
    <mxCell id="1" parent="0"/>
    <object label="vm-01&#10;虛擬機" loomId="n-vm" loomKind="deploymentNode" id="n-vm">
      <mxCell style="rounded=0;" vertex="1" parent="1"><mxGeometry x="0" y="0" width="180" height="60" as="geometry"/></mxCell>
    </object>
    <object label="查快取" loomId="conn-1" loomKind="connection" id="conn-1:a:b">
      <mxCell style="edgeStyle=orthogonalEdgeStyle;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"/></mxCell>
    </object>
    <mxCell id="note" value="&lt;b&gt;Apache&lt;/b&gt;&lt;br&gt;叢集" style="rounded=1;" vertex="1" parent="1">
      <mxGeometry x="0" y="0" width="120" height="60" as="geometry"/>
    </mxCell>
  </root></mxGraphModel>`

  const model = () => {
    const m = new GraphDataModel()
    readXml(m, XML)
    return m
  }

  it('已經指定過的形狀也找得到', () => {
    // 「還沒指定」那份清單刻意只收沒綁的，但搜尋要找的是圖上**所有**東西——
    // 而使用者想找的多半正是已經綁好的那些機器。
    expect(matching(shapeLabels(model()), 'vm-01').map((s) => s.cell)).toEqual(['n-vm'])
  })

  it('使用者自己畫的框也找得到，而且 HTML 標籤不算在字裡', () => {
    // 標籤可以是 HTML，`<b>Apache</b>` 搜「Apache」要找得到、搜「b」不該中。
    expect(matching(shapeLabels(model()), 'Apache').map((s) => s.cell)).toEqual(['note'])
  })
})

describe('把形狀移到正中間', () => {
  const box = { width: 800, height: 600 }

  it('形狀本來就在中間就不用動', () => {
    expect(centered({ x: 0, y: 0 }, { x: 400, y: 300 }, box, 1)).toEqual({ x: 0, y: 0 })
  })

  it('形狀在右下角就往左上移', () => {
    expect(centered({ x: 0, y: 0 }, { x: 800, y: 600 }, box, 1)).toEqual({ x: -400, y: -300 })
  })

  it('放大之後移動的圖座標要跟著除以縮放', () => {
    // `translate` 是圖座標，而形狀位置與容器尺寸都是螢幕像素。
    // 少了這一步，放大兩倍之後會移過頭一倍——而「移過頭」在一張大圖上
    // 看起來跟「沒動」差不多，不會有人發現算錯了。
    expect(centered({ x: 0, y: 0 }, { x: 800, y: 600 }, box, 2)).toEqual({ x: -200, y: -150 })
  })

  it('本來就有平移時是疊上去，不是取代', () => {
    expect(centered({ x: 30, y: 40 }, { x: 800, y: 600 }, box, 1)).toEqual({ x: -370, y: -260 })
  })
})
