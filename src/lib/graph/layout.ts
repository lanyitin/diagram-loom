/**
 * 自己接 elkjs：形狀清單 → 座標。
 *
 * # 為什麼從 draw.io 的排版換成自己接
 *
 * 以前這段是 draw.io 的 `layout` action 做的。換掉 iframe 之後那個沒有了，
 * 但底下本來就是同一個 elkjs——原型實測過，同一組選項跑出同一個等級的結果
 * （見 `docs/maxgraph-findings.md`）。
 *
 * 選項照抄 `docs/nested-layout.md` 實測最好的那一組。**只有 `layered` 能用**：
 * 跨階層的邊與正交繞線是它專屬的能力，其他演算法補不回來。
 *
 * # ⚠️ 轉彎點一定要跟著回來
 *
 * ELK 算完會給每條邊一串 `bendPoints`。不寫回去的話 maxGraph 會自己再繞一次，
 * 而它不知道容器在哪——實測線會**直接穿過三層容器**。看起來像排版很爛，
 * 其實是我們把排版的成果丟掉了（`prototypes/maxgraph/out/no-bends.png`）。
 */

import ELK, { type ElkExtendedEdge, type ElkNode } from 'elkjs/lib/elk.bundled.js'

import type { Link } from '../model'
import type { Shape } from '../diagram'

/** 實測最好的那一組（`docs/nested-layout.md`）。 */
export const OPTIONS: Record<string, string> = {
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

/** 一個節點排完之後的樣子。座標**相對於父節點**，跟 maxGraph 一致。 */
export interface Placed {
  id: string
  x: number
  y: number
  width: number
  height: number
  children: Placed[]
}

/**
 * 一條線在排版結果裡的鍵。
 *
 * ⚠️ **`index` 一律是 `links` 原始陣列裡的位置**，不是過濾之後的位置。
 *
 * 這裡錯過一次，而且很難看出來：排版是「先把畫不出來的線丟掉、再編號」，
 * 取回轉彎點的 [`render`](./render.ts) 卻是拿丟掉**之前**的位置去查。
 * 只要有一條線被丟掉，後面每一條都錯開一格——查不到就等於沒有轉彎點，
 * maxGraph 會自己重繞一次，而它不知道容器在哪。
 *
 * 症狀是「線亂繞、直接穿過容器」，看起來像排版引擎爛，
 * 其實是我們把算好的結果丟掉了。所以這個鍵**只有一個地方寫得出來**。
 */
export function edgeId(link: Link, index: number): string {
  return `${link.connection}#${index}`
}

/** 一條邊排完之後的轉彎點，座標是絕對的（相對於整張圖）。 */
export interface Routed {
  id: string
  bends: { x: number; y: number }[]
}

export interface Laid {
  nodes: Placed[]
  edges: Routed[]
  width: number
  height: number
  /** 排版花了多久。畫面拿它顯示，也拿它判斷要不要提醒使用者。 */
  ms: number
}

/** 形狀的預設大小。實際大小由 ELK 依內容調整（容器）或維持（葉節點）。 */
const SIZE = { width: 180, height: 60 }

const elk = new ELK()

/**
 * 排版。
 *
 * 跨階層的邊一律掛在 root：`INCLUDE_CHILDREN` 會把整棵樹攤平來算，
 * 所以邊放在哪一層都算得出來，而放在 root 我們就不必自己找最近共同祖先。
 */
export async function layout(shapes: Shape[], links: Link[] = []): Promise<Laid> {
  const drawable = new Set(shapes.map((s) => s.id))
  const children = new Map<string | undefined, Shape[]>()
  for (const shape of shapes) {
    const key = shape.parent && drawable.has(shape.parent) ? shape.parent : undefined
    if (!children.has(key)) children.set(key, [])
    children.get(key)!.push(shape)
  }

  const build = (shape: Shape): ElkNode => {
    const kids = (children.get(shape.id) ?? []).map(build)
    return {
      id: shape.id,
      ...SIZE,
      ...(kids.length ? { children: kids } : {}),
      // 容器的尺寸交給 ELK 算。給了固定尺寸又不讓它算，容器就包不住子節點。
      ...(kids.length
        ? { layoutOptions: { 'elk.nodeSize.constraints': 'NODE_LABELS MINIMUM_SIZE' } }
        : {}),
    }
  }

  const graph: ElkNode = {
    id: 'root',
    layoutOptions: OPTIONS,
    children: (children.get(undefined) ?? []).map(build),
    // 兩端都畫得出來的線才送進去。指向不存在的節點會讓 ELK 整個排不出來。
    //
    // 編號要在**過濾之前**取——見 `edgeId`。所以先配對再過濾，
    // 不能直接對過濾完的陣列 `.map((l, i) => …)`。
    edges: links
      .map((link, index) => ({ link, index }))
      .filter(({ link }) => drawable.has(link.from) && drawable.has(link.to))
      .map(({ link, index }) => ({
        id: edgeId(link, index),
        sources: [link.from],
        targets: [link.to],
      })),
  }

  const started = performance.now()
  const laid = await elk.layout(graph)

  return {
    nodes: (laid.children ?? []).map(placed),
    edges: ((laid.edges ?? []) as ElkExtendedEdge[]).map((e) => ({
      id: String(e.id),
      bends: (e.sections ?? []).flatMap((s) => s.bendPoints ?? []).map((p) => ({ x: p.x, y: p.y })),
    })),
    width: laid.width ?? 0,
    height: laid.height ?? 0,
    ms: performance.now() - started,
  }
}

function placed(node: ElkNode): Placed {
  return {
    id: String(node.id),
    x: node.x ?? 0,
    y: node.y ?? 0,
    width: node.width ?? SIZE.width,
    height: node.height ?? SIZE.height,
    children: (node.children ?? []).map(placed),
  }
}

/** 走訪排完的樹，`fn(node, parent)`。父節點是 `null` 表示它在最外層。 */
export function walk(
  nodes: Placed[],
  fn: (node: Placed, parent: Placed | null) => void,
  parent: Placed | null = null,
): void {
  for (const node of nodes) {
    fn(node, parent)
    walk(node.children, fn, node)
  }
}
