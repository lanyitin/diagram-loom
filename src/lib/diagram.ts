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
 * # 一個模型元素 ↔ 一個**形狀**（但線不是）
 *
 * 對帳是集合比對（`reconcile::DiagramElement` 只有 `{id, label}`），
 * 所以同一個 `loomId` 不能出現在兩個**形狀**上。
 *
 * **線是例外。** 一條萬用字元連線（`apache-*` → `gateway-*`）主張的是
 * 「每一台都連得到每一台」，所以圖上是 N×M 條線，全部帶同一個 `loomId`。
 * 這不是偷懶——`lint` 的可達性 BFS 就是這樣建圖的，畫一條反而是說謊
 * （見 `wiring.rs`）。因此 [`boundShapes`] 會**去重**之後才交給 Rust，
 * 對帳那邊看到的仍然是「一條連線一個 id」。
 */

import type { DeploymentNode, Environment, Link, Project } from './model'

/** 一個要畫出來的形狀。 */
interface Shape {
  /** XML 裡的 cell id。線靠它接上來，所以每個形狀都要有。 */
  id: string
  /**
   * 綁到哪個模型元素。**只有環境層的元素有**。
   *
   * 「人」沒有落地，不在 `reconcile::model_elements` 裡；給它 `loomId`
   * 的話對帳會說「圖上有這個、模型沒有」——一個假的缺漏。
   */
  loomId?: string
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
  person: 'shape=umlActor;verticalLabelPosition=bottom;verticalAlign=top;html=1;',
} as const

/** 線的 style。備援線要看得出來，不然圖上四條線一樣重，讀不出主路徑。 */
const EDGE = {
  primary: 'edgeStyle=orthogonalEdgeStyle;rounded=0;html=1;',
  fallback: 'edgeStyle=orthogonalEdgeStyle;rounded=0;html=1;dashed=1;strokeColor=#999999;',
} as const

/** XML 的文字跳脫。少了它，名字裡一個 `&` 就會讓整張圖讀不進去。 */
function esc(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

/**
 * 「人」的形狀。
 *
 * 只畫**真的被這個環境的連線用到的**人。人住在邏輯層、沒有落地，
 * 所以不能無條件全部畫上去——那會在部署圖上出現一堆跟這個環境無關的角色。
 *
 * 刻意**不給 `loomId`**：人不在 `reconcile::model_elements` 裡，
 * 給了對帳就會說「圖上有這個、模型沒有」，變成一個假的缺漏。
 */
export function peopleOf(project: Project, links: Link[]): Shape[] {
  const used = new Set(links.filter((l) => l.fromPerson).map((l) => l.from))
  return (project.logical.people ?? [])
    .filter((p) => used.has(p.id))
    .map((p) => ({ id: p.id, kind: 'person', label: p.slug, style: STYLE.person }))
}

/** 收集一個環境裡所有該畫的形狀。 */
export function shapesOf(project: Project, environment: Environment): Shape[] {
  const shapes: Shape[] = []

  const walk = (nodes: DeploymentNode[], parent?: string) => {
    for (const n of nodes) {
      shapes.push({
        id: n.id,
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
          id: i.id,
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
      id: n.id,
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
      id: s.id,
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
export function toXml(project: Project, environment: Environment, links: Link[] = []): string {
  const shapes = [...shapesOf(project, environment), ...peopleOf(project, links)]

  const cells = shapes
    .map((s) => {
      const label = s.detail ? `${esc(s.label)}&#10;${esc(s.detail)}` : esc(s.label)
      // `loomKind` 標「這個形狀是我們畫的」，`loomId` 標「它是哪個模型元素」。
      // 兩者分開才有辦法畫出「人」——它是我們畫的，但不是環境層的元素。
      const bind = s.loomId ? ` loomId="${esc(s.loomId)}"` : ''
      return [
        `        <object label="${label}"${bind} loomKind="${s.kind}" id="${esc(s.id)}">`,
        `          <mxCell style="${s.style}" vertex="1" parent="${esc(s.parent ?? '1')}">`,
        `            <mxGeometry x="0" y="0" width="180" height="60" as="geometry"/>`,
        `          </mxCell>`,
        `        </object>`,
      ].join('\n')
    })
    .join('\n')

  const drawable = new Set(shapes.map((s) => s.id))
  const edges = links
    // 兩端都要畫得出來。指向不存在的形狀的線在 draw.io 裡會變成一條
    // 飄在畫布上的浮線，看起來像連到別的地方——比不畫更糟。
    .filter((l) => drawable.has(l.from) && drawable.has(l.to))
    .map((l) => {
      // 一條萬用字元連線長出多條線，所以 XML 的 id 要帶上是哪一對，
      // 而 `loomId` 仍然是那一條連線——對帳認的是後者。
      const id = `${l.connection}:${l.from}:${l.to}`
      const style = l.kind === 'fallback' ? EDGE.fallback : EDGE.primary
      return [
        `        <object label="${esc(l.purpose)}" loomId="${esc(l.connection)}" loomKind="connection" id="${esc(id)}">`,
        `          <mxCell style="${style}" edge="1" parent="1" source="${esc(l.from)}" target="${esc(l.to)}">`,
        `            <mxGeometry relative="1" as="geometry"/>`,
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
    edges,
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
  // 依 id 去重。一條萬用字元連線在圖上是 N×M 條線、共用一個 `loomId`，
  // 而對帳那邊的模型側每條連線只有一個元素——不去重的話同一條連線會被
  // 數很多次，衝突判斷跟著錯。
  const out = new Map<string, { id: string; label: string }>()
  // 用 DOMParser 而不是正規表示式：XML 的跳脫與屬性順序都不是正規語言，
  // 而讀錯一個 id 的後果是對帳說「圖上少了這個」——一個假的缺漏。
  const doc = new DOMParser().parseFromString(xml, 'text/xml')
  for (const el of Array.from(doc.querySelectorAll('[loomId]'))) {
    const id = el.getAttribute('loomId')
    if (!id || out.has(id)) continue
    // 標籤裡有換行（名字＋副標），對帳只要第一行。
    const label = (el.getAttribute('label') ?? '').split('\n')[0]!.trim()
    out.set(id, { id, label })
  }
  return [...out.values()]
}

/** 調暗的透明度。25% 淡到不會搶，但還看得出那裡有東西。 */
const DIM = 25

/**
 * 把不在 `lit` 裡的形狀與線調暗。
 *
 * # 這支螢光筆只有一個顏色
 *
 * **只動 `opacity`，其他樣式一個字都不改。** style 字串裡還住著使用者自己
 * 調的顏色、框線、字體——界線講死了，我們才可以放心反覆重寫它，不必記
 * 「它原本是多少」（見 `docs/drawio-integration.md`）。
 *
 * # 不刪任何東西
 *
 * 調暗不是隱藏。隱藏等於把課本剪掉，剪過的課本永遠回答不了「有沒有漏字」；
 * 調暗之後**圖上的形狀一個都沒少**，所以對帳完全不受篩選影響。
 *
 * # 為什麼吃 XML 而不是重新產圖
 *
 * 從模型重產會洗掉使用者手工排好的版面——按一下篩選，半小時的拖拉就沒了。
 * 座標本來就寫在傳進來的這份 XML 裡，改完再 `load` 回去，一個都不會跑掉。
 *
 * 使用者自己畫的裝飾（沒有 `loomKind`）不碰：那不是我們的東西。
 */
export function dim(xml: string, lit: Set<string>): string {
  const doc = new DOMParser().parseFromString(xml, 'text/xml')

  for (const el of Array.from(doc.querySelectorAll('[loomKind]'))) {
    // 線的 `loomId` 是連線 id，形狀的是元素 id；人沒有 `loomId`，
    // 用 cell id 認（`Highlight.shapes` 裡放的就是人的 id）。
    const key = el.getAttribute('loomId') ?? el.getAttribute('id') ?? ''
    const cell = el.querySelector('mxCell') ?? el
    cell.setAttribute('style', withOpacity(cell.getAttribute('style') ?? '', lit.has(key)))
  }

  return new XMLSerializer().serializeToString(doc)
}

/** 換掉 style 裡的 `opacity`，其餘原樣保留。 */
function withOpacity(style: string, isLit: boolean): string {
  const kept = style
    .split(';')
    .filter((part) => part !== '' && !part.startsWith('opacity='))
    .join(';')
  // 亮的就把 opacity 拿掉，而不是設成 100——留著的話使用者自己調的
  // 半透明會被我們永久蓋掉。
  return isLit ? `${kept};` : `${kept};opacity=${DIM};`
}
