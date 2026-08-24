/**
 * 折疊。
 *
 * # 這裡真正在守的兩件事
 *
 * 1. **沒有人收東西的時候，一條線都不准合併。** 一條萬用字元契約本來就是
 *    N×M 條線，合起來等於把「其中一台沒接上」藏掉——那正是這個工具要抓的。
 * 2. **收起來藏了幾個一定要算得出來。** 藏東西跟這個專案的目的是相反的，
 *    所以畫面要講得出「這不是全部」。
 */

import { describe, expect, it } from 'vitest'
import { fold, foldableOf } from './fold'
import { STYLE } from '../diagram'
import type { Shape } from '../diagram'
import type { Link } from '../model'

/** 站點 ⊃ 兩台機器，各跑一個服務。外面還有一台 F5。 */
function shapes(): Shape[] {
  return [
    { id: 'site', loomId: 'site', kind: 'deploymentNode', label: 'dc-main', style: STYLE.site },
    { id: 'vm-a', loomId: 'vm-a', kind: 'deploymentNode', label: 'vm-a', parent: 'site', style: STYLE.node },
    { id: 'a1', loomId: 'a1', kind: 'containerInstance', label: 'api-01', parent: 'vm-a', style: STYLE.instance },
    { id: 'vm-b', loomId: 'vm-b', kind: 'deploymentNode', label: 'vm-b', parent: 'site', style: STYLE.node },
    { id: 'b1', loomId: 'b1', kind: 'containerInstance', label: 'api-02', parent: 'vm-b', style: STYLE.instance },
    { id: 'f5', loomId: 'f5', kind: 'infrastructureNode', label: 'f5-01', style: STYLE.infra },
  ]
}

const link = (over: Partial<Link>): Link => ({
  connection: 'c1', from: 'a1', to: 'f5', fromPerson: false, kind: 'primary', purpose: '查快取', ...over,
} as Link)

const ids = (list: { id: string }[]) => list.map((s) => s.id)

describe('沒有人收東西', () => {
  it('形狀與線原封不動', () => {
    const links = [link({}), link({ from: 'b1' })]
    const out = fold(shapes(), links, new Set())
    expect(ids(out.shapes)).toEqual(['site', 'vm-a', 'a1', 'vm-b', 'b1', 'f5'])
    expect(out.links).toEqual(links)
  })

  it('同一對之間的正常線與備援線都留著', () => {
    // 合成一條的話備援線就不見了，而且沒有任何訊息。
    const links = [link({ connection: 'c1' }), link({ connection: 'c2', kind: 'fallback' })]
    expect(fold(shapes(), links, new Set()).links).toHaveLength(2)
  })

  it('底下有東西的框標成收得起來', () => {
    const out = fold(shapes(), [], new Set())
    expect(out.shapes.filter((s) => s.foldable).map((s) => s.id)).toEqual(['site', 'vm-a', 'vm-b'])
  })
})

describe('收起一個框', () => {
  const collapsed = new Set(['vm-a'])

  it('底下的東西不畫', () => {
    const out = fold(shapes(), [], collapsed)
    expect(ids(out.shapes)).toEqual(['site', 'vm-a', 'vm-b', 'b1', 'f5'])
  })

  it('線改接到收起來的那個框', () => {
    const out = fold(shapes(), [link({ from: 'a1', to: 'f5' })], collapsed)
    expect(out.links).toHaveLength(1)
    expect(out.links[0]!.from).toBe('vm-a')
  })

  it('框上寫出藏了幾個', () => {
    // 藏東西跟這個工具的目的是相反的，所以它一定要看得見。
    const out = fold(shapes(), [], collapsed)
    expect(out.shapes.find((s) => s.id === 'vm-a')!.detail).toContain('收起 1 個')
  })

  it('收起來的框仍然標成收得起來，不然打不開', () => {
    // 它現在沒有小孩了。問 maxGraph「有小孩嗎」會得到零，± 圖示就消失。
    const out = fold(shapes(), [], collapsed)
    expect(out.shapes.find((s) => s.id === 'vm-a')!.foldable).toBe(true)
    expect(out.shapes.find((s) => s.id === 'vm-a')!.collapsed).toBe(true)
  })
})

describe('線的收攏', () => {
  it('一條契約長出來的多條線收成一條，而且還綁著那條契約', () => {
    // 萬用字元的 N×M 條線共用同一個 loomId，合起來仍然是那一條契約。
    const links = [
      link({ connection: 'c1', from: 'a1', to: 'f5' }),
      link({ connection: 'c1', from: 'b1', to: 'f5' }),
    ]
    const out = fold(shapes(), links, new Set(['site']))
    expect(out.links).toHaveLength(1)
    expect(out.links[0]!.connection).toBe('c1')
    expect(out.links[0]!.purpose).toBe('查快取')
  })

  it('好幾條契約收成一條時寫數量，而且刻意沒有 loomId', () => {
    // 一條線代表一群契約，指其中一條等於說謊，而對帳會把綁定當事實。
    const links = [
      link({ connection: 'c1', from: 'a1', to: 'f5', purpose: '查快取' }),
      link({ connection: 'c2', from: 'b1', to: 'f5', purpose: '寫紀錄' }),
    ]
    const out = fold(shapes(), links, new Set(['site']))
    expect(out.links).toHaveLength(1)
    expect(out.links[0]!.purpose).toBe('2 條連線')
    expect(out.links[0]!.connection).toBe('')
  })

  it('混著備援時畫成正常線', () => {
    // 全部畫成備援的話，主路徑在圖上就看不見了。
    const links = [
      link({ connection: 'c1', from: 'a1', to: 'f5', kind: 'primary' }),
      link({ connection: 'c2', from: 'b1', to: 'f5', kind: 'fallback' }),
    ]
    expect(fold(shapes(), links, new Set(['site'])).links[0]!.kind).toBe('primary')
  })

  it('兩端收進同一個框裡的線不畫', () => {
    // 那條線變成一個圈，讀不出任何東西。
    const links = [link({ from: 'a1', to: 'b1' })]
    expect(fold(shapes(), links, new Set(['site'])).links).toEqual([])
  })

  it('沒被收攏到的線不會被順便合併', () => {
    // vm-a 收起來了，但 b1 → f5 兩端都沒動，它的正常線與備援線要各留一條。
    const links = [
      link({ connection: 'c1', from: 'b1', to: 'f5' }),
      link({ connection: 'c2', from: 'b1', to: 'f5', kind: 'fallback' }),
    ]
    expect(fold(shapes(), links, new Set(['vm-a'])).links).toHaveLength(2)
  })
})

describe('收起來的框裡面又有收起來的框', () => {
  it('由外面那個代表，不是裡面那個', () => {
    // 由內往外找會找到 vm-a，而它自己也是藏著的——線會接到一個看不見的框上。
    const out = fold(shapes(), [link({ from: 'a1', to: 'f5' })], new Set(['site', 'vm-a']))
    expect(ids(out.shapes)).toEqual(['site', 'f5'])
    expect(out.links[0]!.from).toBe('site')
  })

  it('藏了幾個算的是全部的子孫', () => {
    const out = fold(shapes(), [], new Set(['site']))
    expect(out.shapes.find((s) => s.id === 'site')!.detail).toContain('收起 4 個')
  })
})

describe('哪些框收得起來', () => {
  it('底下有東西的才算', () => {
    expect(foldableOf(shapes())).toEqual(new Set(['site', 'vm-a', 'vm-b']))
  })
})
