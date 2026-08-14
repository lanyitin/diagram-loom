/**
 * 自己接 elkjs：模型 → ELK 圖 → 座標。
 *
 * draw.io 那邊這一段是它內建的（`layout` action），換成 maxGraph 就得自己寫。
 * **這個檔案就是「自己接要付多少」的答案**——它有多長，代價就是多少。
 *
 * 選項照抄 `docs/nested-layout.md` 實測最好的那一組。那組是在 draw.io 內建的
 * ELK 上調出來的，而底下是同一個 elkjs——所以**這裡不是重調參數，是驗證
 * 同樣的參數在我們自己手上跑出同樣的結果**。
 */

import ELK from 'elkjs/lib/elk.bundled.js'

/** 實測最好的那一組（`docs/nested-layout.md`）。 */
export const OPTIONS = {
  'elk.algorithm': 'layered',
  'elk.direction': 'RIGHT',
  // 這兩個是關鍵。少了它們，跨容器的邊與正交繞線都沒有——
  // 而且只有 layered 吃得下（實測補給 mrtree / force 完全沒有作用）。
  'elk.edgeRouting': 'ORTHOGONAL',
  'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
  // 容器標題的位子。少了上方留白，標題會壓到第一個子節點。
  'elk.padding': '[top=44,left=16,bottom=16,right=16]',
  'elk.spacing.nodeNode': '40',
  'elk.layered.spacing.nodeNodeBetweenLayers': '60',
}

const elk = new ELK()

/** 模型 → ELK 的 JSON 圖。 */
function toElk(node) {
  const children = (node.children ?? []).map(toElk)
  return {
    id: node.id,
    width: node.width,
    height: node.height,
    ...(children.length ? { children } : {}),
    // 容器的尺寸交給 ELK 算。給了原始尺寸又不讓它算，就會像 mrtree 那樣
    // 維持作者寫的寬度、包不住子節點（`docs/nested-layout.md` 的 L3）。
    ...(children.length
      ? { layoutOptions: { 'elk.nodeSize.constraints': 'NODE_LABELS MINIMUM_SIZE' } }
      : {}),
  }
}

/**
 * 排版。
 *
 * 跨階層的邊一律掛在 root：`INCLUDE_CHILDREN` 會把整棵樹攤平來算，
 * 所以邊放在哪一層都算得出來，而放在 root 我們就不必自己找最近共同祖先。
 *
 * `fixed` 裡的節點會被釘在原地（`docs/nested-layout.md` 的 L4 說 draw.io
 * 做不到這件事，這裡要驗證自己接就做得到）。
 */
export async function layout(model, { fixed = {}, options = {} } = {}) {
  const graph = {
    id: 'root',
    layoutOptions: { ...OPTIONS, ...options },
    children: model.nodes.map(toElk),
    edges: model.edges.map((e) => ({ id: e.id, sources: [e.from], targets: [e.to] })),
  }

  pin(graph.children, fixed)

  const started = performance.now()
  const laid = await elk.layout(graph)
  return { laid, ms: performance.now() - started }
}

/** 把指定的節點釘在原地。 */
function pin(nodes, fixed) {
  for (const node of nodes) {
    const at = fixed[node.id]
    if (at) {
      node.layoutOptions = {
        ...(node.layoutOptions ?? {}),
        'elk.position': `(${at.x},${at.y})`,
      }
    }
    pin(node.children ?? [], fixed)
  }
}

/** 走訪排版結果，`fn(node, parent)`。 */
export function walk(node, fn, parent = null) {
  for (const child of node.children ?? []) {
    fn(child, parent ?? node)
    walk(child, fn, child)
  }
}

/** 依 id 找排完的節點。 */
export function find(laid, id) {
  let found = null
  walk(laid, (node) => {
    if (node.id === id) found = node
  })
  return found
}
