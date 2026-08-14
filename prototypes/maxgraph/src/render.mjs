/**
 * 把排完版的圖畫進 maxGraph。
 *
 * 兩支原型（`app.mjs` 與 `editor/`）共用這一份，因為裡面有**一段一定會忘記
 * 的邏輯**：ELK 算好的轉彎點要寫回邊上。抄兩份的話，遲早有一邊漏掉，
 * 而漏掉的症狀是「排版看起來很爛」，不是「這裡少寫了一段」。
 */

import { Point } from '@maxgraph/core'

/**
 * `laid` 是 elkjs 排完的結果，`model` 是原始模型（要拿它的邊）。
 * `styleOf(node)` 決定每個節點長什麼樣。回傳 id → cell 的對照表。
 */
export function render(graph, laid, model, styleOf) {
  const cells = new Map()

  graph.batchUpdate(() => {
    const place = (node, parent) => {
      const cell = graph.insertVertex({
        parent,
        id: node.id,
        value: node.id,
        // ELK 的座標是相對於父節點的，maxGraph 也是——直接放，不必換算。
        position: [node.x, node.y],
        size: [node.width, node.height],
        style: styleOf(node),
      })
      cells.set(node.id, cell)
      for (const child of node.children ?? []) place(child, cell)
    }
    for (const node of laid.children ?? []) place(node, graph.getDefaultParent())

    const routes = new Map((laid.edges ?? []).map((e) => [e.id, e]))
    for (const edge of model.edges) {
      const bends = (routes.get(edge.id)?.sections ?? []).flatMap((s) => s.bendPoints ?? [])
      const cell = graph.insertEdge({
        parent: graph.getDefaultParent(),
        id: edge.id,
        source: cells.get(edge.from),
        target: cells.get(edge.to),
        // ⚠️ 有轉彎點時**不要再給 edgeStyle**：ELK 給的已經是完整的正交折線，
        // 再套一個繞線器等於兩個東西同時決定線怎麼走，而贏的是不知道容器
        // 在哪的那個——實測線會直接穿過三層容器（見 out/no-bends.png）。
        style: bends.length ? { rounded: false } : { edgeStyle: 'orthogonalEdgeStyle' },
      })
      if (bends.length) {
        // ⚠️ 直接改 `getGeometry().points` 不會產生變更紀錄，畫面不知道要重畫。
        const geometry = cell.getGeometry().clone()
        geometry.points = bends.map((p) => new Point(p.x, p.y))
        graph.model.setGeometry(cell, geometry)
      }
    }
  })

  return cells
}
