/**
 * 畫布**什麼時候**重畫。
 *
 * 這裡守的不是「畫得對不對」（那在 `lib/graph/render.test.ts`），
 * 而是「重畫的時機」——兩個坑都不會報錯，只會讓畫面停在錯的那一版：
 *
 * 1. 連線比形狀晚到。沒有盯著連線的話，畫布會停在「零條線」那次的排版，
 *    而零條邊的 `layered` 排版是把所有形狀疊成一直條。
 * 2. 排版是非同步的，兩次重畫會同時在跑。先算完的蓋在後算完的上面，
 *    畫面就停在舊資料上。
 */

import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import type { Cell } from '@maxgraph/core'
import DiagramCanvas from './DiagramCanvas.vue'
import { STYLE } from '../lib/diagram'
import type { Shape } from '../lib/diagram'
import type { Link } from '../lib/model'

function shapes(count: number): Shape[] {
  return Array.from({ length: count }, (_, i) => ({
    id: `svc-${i}`,
    loomId: `svc-${i}`,
    kind: 'containerInstance',
    label: `svc-${i}`,
    style: STYLE.instance,
  }))
}

const links: Link[] = [
  { connection: 'c1', from: 'svc-0', to: 'svc-1', fromPerson: false, kind: 'primary', purpose: '查快取' },
] as Link[]

/** 一份使用者存過的圖：只有一個框，而且它的 id 跟產生出來的都不一樣。 */
const SAVED = `<mxGraphModel><root>
  <mxCell id="0"/><mxCell id="1" parent="0"/>
  <object label="手排的" loomId="mine" id="m1">
    <mxCell style="" vertex="1" parent="1">
      <mxGeometry x="0" y="0" width="80" height="40" as="geometry"/>
    </mxCell>
  </object>
</root></mxGraphModel>`

function canvas(props: { shapes: Shape[]; links: Link[] }) {
  return mount(DiagramCanvas, { props: { ...props, xml: null } })
}

/** 畫布上目前有哪些 cell。`draw()` 每次都重建整棵樹，所以直接數就好。 */
function cellsOf(w: ReturnType<typeof canvas>): Cell[] {
  const graph = (w.vm as unknown as { graph: { getDataModel(): { getRoot(): Cell } } }).graph
  return graph.getDataModel().getRoot()?.children?.[0]?.children ?? []
}

/** 等排版跑完。elkjs 是非同步的，而它跑完之後還要再等一輪才進得了模型。 */
const settle = () => new Promise((resolve) => setTimeout(resolve, 300))

describe('重畫的時機', () => {
  it('連線比形狀晚到，線還是會畫上去', async () => {
    // 連線是另一支 Tauri command 拿的，一定比形狀晚。畫布沒盯著它的話，
    // 畫面就停在零條線那次的排版——所有形狀排成一直條，看起來像排版壞了。
    const w = canvas({ shapes: shapes(3), links: [] })
    await settle()
    expect(cellsOf(w).filter((c) => c.isEdge()), '一開始就不該有線').toHaveLength(0)

    await w.setProps({ links })
    await settle()
    expect(cellsOf(w).filter((c) => c.isEdge()), '連線到了卻沒重畫').toHaveLength(1)
  })

  it('排版還在跑的時候換去讀存過的圖，讀進來的不會被蓋掉', async () => {
    // 兩條路的速度差很多：讀檔是同步的，排版要等 elkjs。
    // 使用者從「App 產的詳圖」切到「自己排過版的圖」時，讀檔那次會先畫完，
    // 排版那次晚一步落地就把它蓋掉——他看到的是自動排的版，
    // 而他手排的座標其實好端端在檔案裡。畫面上沒有任何訊息說明發生了什麼。
    const w = canvas({ shapes: shapes(60), links: [] })
    await w.setProps({ xml: SAVED })
    await settle()

    const ids = cellsOf(w).map((c) => c.id)
    expect(ids, '存過的圖沒讀進來').toContain('m1')
    expect(ids.filter((id) => id?.startsWith('svc-')), '排版那次蓋掉了讀進來的圖').toEqual([])
  })
})
