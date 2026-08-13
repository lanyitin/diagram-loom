/**
 * 資源表的排序與篩選。
 *
 * 這裡真正在守的是**機器那張表是一棵樹**。排序把樹拆散、篩選把子節點變孤兒，
 * 兩者的症狀都一樣糟：縮排還在，但它在說謊——畫面顯示一個錯的從屬關係。
 */

import { describe, expect, it } from 'vitest'
import { filterRows, nextSort, sortRows } from './rows'
import type { ResourceRow } from './model'

function row(cells: string[], depth = 0): ResourceRow {
  return { id: cells[0]!, cells, depth, severity: null, resource: {} } as unknown as ResourceRow
}

/** 站點底下兩台機器，其中一台底下還有一台。 */
function tree(): ResourceRow[] {
  return [
    row(['dc-taipei'], 0),
    row(['vm-10'], 1),
    row(['ct-a'], 2),
    row(['vm-2'], 1),
    row(['dc-backup'], 0),
    row(['vm-9'], 1),
  ]
}

/** 這幾張假表都只有一欄，就叫「名稱」。 */
const COLUMNS = ['名稱']

const names = (rows: ResourceRow[]) => rows.map((r) => r.cells[0])
const shape = (rows: ResourceRow[]) => rows.map((r) => `${'  '.repeat(r.depth)}${r.cells[0]}`)

describe('排序', () => {
  it('平的表就照那一欄排', () => {
    const rows = [row(['redis']), row(['apache']), row(['gateway'])]
    expect(names(sortRows(rows, { column: '名稱', direction: 'asc' }, COLUMNS)))
      .toEqual(['apache', 'gateway', 'redis'])
  })

  it('倒過來排', () => {
    const rows = [row(['redis']), row(['apache']), row(['gateway'])]
    expect(names(sortRows(rows, { column: '名稱', direction: 'desc' }, COLUMNS)))
      .toEqual(['redis', 'gateway', 'apache'])
  })

  it('編號照數字排，不照字串排', () => {
    // 不然 vm-10 會排在 vm-2 前面，而機器名字幾乎都是這種帶編號的。
    const rows = [row(['vm-10']), row(['vm-2']), row(['vm-1'])]
    expect(names(sortRows(rows, { column: '名稱', direction: 'asc' }, COLUMNS)))
      .toEqual(['vm-1', 'vm-2', 'vm-10'])
  })

  it('只在兄弟之間排，不把樹拆散', () => {
    // 直接 sort 的話 ct-a 會跑到別的父節點底下，而縮排還在——
    // 畫面就會顯示一個錯的從屬關係。那比不能排序糟得多。
    expect(shape(sortRows(tree(), { column: '名稱', direction: 'asc' }, COLUMNS))).toEqual([
      'dc-backup',
      '  vm-9',
      'dc-taipei',
      '  vm-2',
      '  vm-10',
      '    ct-a',
    ])
  })

  it('沒指定就照 Rust 給的原始順序', () => {
    expect(names(sortRows(tree(), null, COLUMNS))).toEqual(names(tree()))
  })
})

describe('篩選', () => {
  it('哪一欄對得上都算', () => {
    const rows = [row(['redis-01', '10.0.0.1']), row(['apache-01', '10.0.0.2'])]
    expect(names(filterRows(rows, '10.0.0.2'))).toEqual(['apache-01'])
  })

  it('不分大小寫', () => {
    expect(names(filterRows([row(['Redis-01'])], 'redis'))).toEqual(['Redis-01'])
  })

  it('空的關鍵字就是不篩', () => {
    expect(filterRows(tree(), '   ')).toHaveLength(6)
  })

  it('子節點對得上時，父節點要留著', () => {
    // 不留的話子節點會變孤兒——縮排還在，但它縮排在誰底下已經沒人知道了。
    expect(shape(filterRows(tree(), 'ct-a'))).toEqual([
      'dc-taipei',
      '  vm-10',
      '    ct-a',
    ])
  })

  it('父節點對得上時，底下的照原樣留著', () => {
    expect(shape(filterRows(tree(), 'dc-backup'))).toEqual(['dc-backup', '  vm-9'])
  })

  it('都對不上就是空的', () => {
    expect(filterRows(tree(), '沒有這種東西')).toEqual([])
  })
})

describe('點欄位標題', () => {
  it('點同一欄在遞增與遞減之間換，第三次回到原始順序', () => {
    // 一定要有辦法回到原始順序：Rust 給的順序是有意義的
    // （機器是樹、契約照定義順序），排過就回不去的話那個資訊就沒了。
    let s = nextSort(null, '名稱')
    expect(s).toEqual({ column: '名稱', direction: 'asc' })
    s = nextSort(s, '名稱')
    expect(s).toEqual({ column: '名稱', direction: 'desc' })
    expect(nextSort(s, '名稱')).toBeNull()
  })

  it('點別欄就從遞增開始', () => {
    expect(nextSort({ column: '名稱', direction: 'desc' }, '種類'))
      .toEqual({ column: '種類', direction: 'asc' })
  })

  it('欄名對不上時原樣回傳，不會排到別欄去', () => {
    // 欄位可以藏起來之後，排序的那一欄有可能已經不在畫面上了。
    // 這時候什麼都不做才對——排到別欄去看起來會像排錯了。
    const rows = [row(['redis']), row(['apache'])]
    expect(names(sortRows(rows, { column: '不存在的欄', direction: 'asc' }, COLUMNS)))
      .toEqual(['redis', 'apache'])
  })
})
