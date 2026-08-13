/**
 * 從模型產出 draw.io 的 XML。
 *
 * # 為什麼在 JavaScript
 *
 * 這是**轉接**不是規則：把已經算好的模型換一種寫法。draw.io 的
 * XML 格式怎麼寫、用哪個 style 字串，都是它那邊的細節（見 CLAUDE.md
 * 的職責分界）。
 *
 * # 每個形狀都要帶 `loomId`
 *
 * 對帳靠的就是它。階段 0 實測過：`<object loomId=...>` 包住 `<mxCell>`
 * 之後，載入、排版兩次、再取回來，UUID 一個沒少
 * （見 `docs/stage0-findings.md` 第五節）。
 *
 * **反過來說：沒有 `loomId` 的形狀，對帳看不見。** 那正好可以拿來畫
 * 純裝飾的東西——但也意味著漏掉一個 `loomId` 不會有人叫，圖上那個
 * 元素就從此跟模型脫鉤了。所以這裡的規矩是：**每一個代表模型元素的
 * 形狀，都必須帶 `loomId`**，由測試守著。
 *
 * # 一個模型元素 ↔ 一個形狀
 *
 * 對帳是集合比對（`reconcile::DiagramElement` 只有 `{id, label}`），
 * 所以同一個 `loomId` 不能出現兩次。畫多台機器的叢集時要記得這條。
 */

import type { DeploymentNode, Environment, Project } from './model'

/** 一個要畫出來的形狀。 */
interface Shape {
  loomId: string
  kind: string
  label: string
  /** 副標，畫在名字下面（位址、種類）。 */
  detail?: string
  /** 巢狀：這個形狀畫在誰裡面。 */
  parent?: string
  style: string
}

/** draw.io 的 style 字串。集中在這裡，不要散在產生邏輯裡。 */
const STYLE = {
  site: 'rounded=0;whiteSpace=wrap;html=1;fillColor=none;dashed=1;verticalAlign=top;fontStyle=1;',
  node: 'rounded=0;whiteSpace=wrap;html=1;fillColor=none;verticalAlign=top;',
  instance: 'rounded=1;whiteSpace=wrap;html=1;',
  infra: 'shape=hexagon;perimeter=hexagonPerimeter2;whiteSpace=wrap;html=1;',
  system: 'rounded=1;whiteSpace=wrap;html=1;dashed=1;',
} as const

/** XML 的文字跳脫。少了它，名字裡一個 `&` 就會讓整張圖讀不進去。 */
function esc(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

/** 收集一個環境裡所有該畫的形狀。 */
export function shapesOf(project: Project, environment: Environment): Shape[] {
  const shapes: Shape[] = []

  const walk = (nodes: DeploymentNode[], parent?: string) => {
    for (const n of nodes) {
      shapes.push({
        loomId: n.id,
        kind: 'deploymentNode',
        label: n.slug,
        // 站點本身不跑東西，所以它不必標種類——它就是一個框。
        detail: n.kind === 'site' ? undefined : kindLabel(n.kind),
        parent,
        style: n.kind === 'site' ? STYLE.site : STYLE.node,
      })

      for (const i of n.instances ?? []) {
        const container = project.logical.containers.find((c) => c.id === i.container)
        shapes.push({
          loomId: i.id,
          kind: 'containerInstance',
          label: i.slug,
          // 位址是這張圖最常被拿去核對的東西，直接畫在上面。
          detail: (i.endpoints ?? []).map((e) => e.address).filter(Boolean).join('、')
            || container?.name,
          parent: n.id,
          style: STYLE.instance,
        })
      }

      walk(n.children ?? [], n.id)
    }
  }
  walk(environment.nodes ?? [])

  for (const n of environment.infra ?? []) {
    shapes.push({
      loomId: n.id,
      kind: 'infrastructureNode',
      label: n.slug,
      detail: (n.endpoints ?? []).map((e) => e.address).filter(Boolean).join('、'),
      style: STYLE.infra,
    })
  }

  for (const s of environment.systems ?? []) {
    const system = project.logical.systems.find((x) => x.id === s.system)
    shapes.push({
      loomId: s.id,
      kind: 'softwareSystemInstance',
      // 外部系統畫成虛線框——讀圖的人要一眼分得出「這不是我們的」。
      label: s.slug,
      detail: system?.name,
      style: STYLE.system,
    })
  }

  return shapes
}

function kindLabel(kind: string): string {
  return (
    { site: '站點', physical: '實體機', 'virtual-machine': '虛擬機', 'linux-container': 'Linux 容器' }[
      kind
    ] ?? kind
  )
}

/**
 * 產出一整份 `.drawio`。
 *
 * 座標全部給 0——**排版交給 draw.io 的 ELK**（`layout` action）。
 * 自己算座標等於重寫一個排版引擎，而它內建的那個已經夠好
 * （見 `docs/nested-layout.md`）。
 */
export function toXml(project: Project, environment: Environment): string {
  const shapes = shapesOf(project, environment)

  const cells = shapes
    .map((s) => {
      const label = s.detail ? `${esc(s.label)}&#10;${esc(s.detail)}` : esc(s.label)
      return [
        `        <object label="${label}" loomId="${esc(s.loomId)}" loomKind="${s.kind}" id="${esc(s.loomId)}">`,
        `          <mxCell style="${s.style}" vertex="1" parent="${esc(s.parent ?? '1')}">`,
        `            <mxGeometry x="0" y="0" width="180" height="60" as="geometry"/>`,
        `          </mxCell>`,
        `        </object>`,
      ].join('\n')
    })
    .join('\n')

  return [
    '<mxfile host="diagram-loom">',
    `  <diagram id="${esc(environment.id)}" name="${esc(environment.slug)}">`,
    '    <mxGraphModel dx="0" dy="0" grid="1" gridSize="10" page="1" pageWidth="1169" pageHeight="827">',
    '      <root>',
    '        <mxCell id="0"/>',
    '        <mxCell id="1" parent="0"/>',
    cells,
    '      </root>',
    '    </mxGraphModel>',
    '  </diagram>',
    '</mxfile>',
  ]
    .filter((line) => line !== '')
    .join('\n')
}

/**
 * 從編輯器取回來的 XML 裡挖出 `[{id, label}]`。
 *
 * 這是對帳的另一半：**JS 說「圖上有什麼」，Rust 說「這代表什麼」**。
 * 所以這裡只挖，不判斷。
 *
 * ⚠️ 取模型一定要用 `format:'xml'`。`xmlsvg` 也附一個 `xml` 欄位，
 * 但那個**永遠是壓縮的**，`loomId` 整個看不見
 * （見 `docs/stage0-findings.md` 第四節——第一輪就是被這個騙到）。
 */
export function boundShapes(xml: string): { id: string; label: string }[] {
  const out: { id: string; label: string }[] = []
  // 用 DOMParser 而不是正規表示式：XML 的跳脫與屬性順序都不是正規語言，
  // 而讀錯一個 id 的後果是對帳說「圖上少了這個」——一個假的缺漏。
  const doc = new DOMParser().parseFromString(xml, 'text/xml')
  for (const el of Array.from(doc.querySelectorAll('[loomId]'))) {
    const id = el.getAttribute('loomId')
    if (!id) continue
    // 標籤裡有換行（名字＋副標），對帳只要第一行。
    const label = (el.getAttribute('label') ?? '').split('\n')[0]!.trim()
    out.push({ id, label })
  }
  return out
}
