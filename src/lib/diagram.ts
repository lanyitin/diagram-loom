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

// `UnboundShape` 用 Rust 產的那份，不自己再定義一個：它是要送過去給
// `annotate` 的東西，兩邊各寫一份的話，改了欄位不會有人叫。
import type {
  DeploymentNode,
  Environment,
  Link,
  Project,
  RelationshipEnd,
  UnboundShape,
} from './model'

/** 一個要畫出來的形狀。 */
export interface Shape {
  /** XML 裡的 cell id。線靠它接上來，所以每個形狀都要有。 */
  id: string
  /**
   * 綁到哪個模型元素。**只有環境層的元素有**。
   *
   * 「人」沒有實體，不在 `reconcile::model_elements` 裡；給它 `loomId`
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
  /**
   * 底下有東西，所以收得起來。畫布拿它決定要不要長出那個 ± 圖示。
   *
   * ⚠️ **不能問「這個 cell 有沒有小孩」**（那是 maxGraph 的預設）：收起來
   * 之後小孩根本沒被畫出去，一問就是零，± 圖示會跟著消失——收得起來、
   * 打不開。所以這件事要由 [`fold`](./graph/fold.ts) 從**完整的**清單算。
   */
  foldable?: boolean
  /** 收起來了。底下的東西不畫，線改接到它身上。 */
  collapsed?: boolean
}

/** draw.io 的 style 字串。集中在這裡，不要散在產生邏輯裡。 */
export const STYLE = {
  site: 'rounded=0;whiteSpace=wrap;html=1;fillColor=none;dashed=1;verticalAlign=top;fontStyle=1;',
  node: 'rounded=0;whiteSpace=wrap;html=1;fillColor=none;verticalAlign=top;',
  instance: 'rounded=1;whiteSpace=wrap;html=1;',
  infra: 'shape=hexagon;perimeter=hexagonPerimeter2;whiteSpace=wrap;html=1;',
  system: 'rounded=1;whiteSpace=wrap;html=1;dashed=1;',
  person: 'shape=umlActor;verticalLabelPosition=bottom;verticalAlign=top;html=1;',
  /** Context 圖的自家系統。字粗一點——這張圖上框就是主角，沒有機器搶戲。 */
  ownedSystem: 'rounded=1;whiteSpace=wrap;html=1;fontStyle=1;',
  /** Context 圖的外部系統。虛線，一眼看得出「這不是我們的」。 */
  externalSystem: 'rounded=1;whiteSpace=wrap;html=1;dashed=1;',
} as const

/** 線的 style。備援線要看得出來，不然圖上四條線一樣重，讀不出主路徑。 */
export const EDGE = {
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
 * 只畫**真的被這個環境的連線用到的**人。人住在邏輯層、沒有實體，
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

/**
 * **畫布上該有的形狀**：環境層的東西，加上被這個環境的連線用到的人。
 *
 * # 為什麼要有這一支，而不是各自 `[...shapesOf(), ...peopleOf()]`
 *
 * 因為「人也要畫」是一件會被忘記的事，而且**忘記不會報錯**：
 *
 * 1. 畫布上沒有人 → 起點是人的那條線兩端對不上 → 被安靜地丟掉
 * 2. 丟掉一條線，後面每一條線的轉彎點就整批錯開（見 `graph/layout.ts` 的 `edgeId`）
 * 3. 拿不到轉彎點，maxGraph 自己重繞，而它不知道容器在哪 → 線穿過所有框
 *
 * 這串真的發生過：換成 maxGraph 之後畫布走 `shapesOf`，而畫人的
 * [`peopleOf`] 只留在 [`toXml`] 那條已經沒人走的路上。拿真實專案跑，
 * **145 條線的轉彎點全部掉光**，整張圖變成蜘蛛網。
 *
 * 所以「畫布上該有什麼」只有這一個答案。
 */
export function canvasShapes(
  project: Project,
  environment: Environment,
  links: Link[],
): Shape[] {
  return [...shapesOf(project, environment), ...peopleOf(project, links)]
}

/** 收集一個環境裡所有該畫的形狀。**人不在裡面**，見 [`canvasShapes`]。 */
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
      // 跟服務實體同一條規矩：有位址就印位址。外部系統的位址是最常被
      // 拿去核對防火牆的東西，印系統名字等於把它藏起來。
      detail: (s.endpoints ?? []).map((e) => e.address).filter(Boolean).join('、')
        || system?.name,
      style: STYLE.system,
    })
  }

  return shapes
}

/** 一張圖要畫的東西：形狀加線。 */
export interface Drawing {
  shapes: Shape[]
  links: Link[]
}

/**
 * **Context 圖**：人與系統，一個系統一個框。沒有機器、沒有服務、沒有位址。
 *
 * # 跟部署圖的三個差別
 *
 * 1. **框是邏輯層的**。部署圖畫「redis-01 跑在 vm-03 上」，這張畫「商店系統」。
 * 2. **線是收攏過的**。A 系統對 B 系統有三條契約，這裡只畫一條。
 * 3. **它不對帳**（`diagrams::kind_of` 標成簡圖）。第 2 點就是原因：
 *    一條線代表一群契約，撐不住「一個 `loomId` ↔ 一個元素」那條規矩。
 *
 * # 為什麼「人」在這裡有 `loomId`，在部署圖上卻沒有
 *
 * 不是不一致，是兩張圖的模型側不一樣（見 `reconcile::elements_for`）：
 * 詳圖比對的是**環境層**，人不在裡面，給了 `loomId` 會變成一個假的缺漏；
 * 簡圖比對的是環境層**加上邏輯層**，人就在裡面，不給反而讓「有人把這個
 * 角色刪了」變成沒有人會發現的事。
 *
 * # 為什麼只畫這個環境部署到的系統
 *
 * 三個環境的 context 圖長得不一樣本身就是資訊：test 沒接的那個外部金流
 * 不該出現在 test 的圖上。判斷依據是**這個環境有沒有它的實體**，
 * 而不是「有沒有連線」——契約在但連線還沒建是 L001 要罵的事，
 * 藏起來等於幫忙把缺漏蓋掉。
 */
export function contextDrawing(project: Project, environment: Environment): Drawing {
  const logical = project.logical
  const systemOf = new Map((logical.containers ?? []).map((c) => [c.id, c.system]))

  // ── 這個環境部署了哪些系統 ──────────────────────────────
  const present = new Set<string>()
  const walk = (nodes: DeploymentNode[]) => {
    for (const n of nodes) {
      for (const i of n.instances ?? []) {
        const system = systemOf.get(i.container)
        if (system) present.add(system)
      }
      walk(n.children ?? [])
    }
  }
  walk(environment.nodes ?? [])
  for (const s of environment.systems ?? []) present.add(s.system)

  // ── 一條契約的一端，在這張圖上是哪個框 ──────────────────
  // 設備（F5）不會出現：契約的兩端只會是人、系統或服務，VIP 是環境層
  // 怎麼接的細節。這也正是線要從**契約**推、不是從連線推的理由——
  // 從連線推的話，prod 走 F5 的那一段會變成「A 連 F5」「F5 連 B」兩條。
  const boxOf = (end: RelationshipEnd): string | null => {
    if (end.person) return end.person
    if (end.system) return present.has(end.system) ? end.system : null
    if (end.container) {
      const system = systemOf.get(end.container)
      return system && present.has(system) ? system : null
    }
    return null
  }

  // ── 線：一對系統收成一條 ────────────────────────────────
  const pairs = new Map<string, { from: string; to: string; of: string[] }>()
  for (const r of logical.relationships ?? []) {
    const from = boxOf(r.from)
    const to = boxOf(r.to)
    // 自己連自己在 context 圖上是一個圈，讀不出任何東西——那是 container
    // 圖的層次（同一個系統裡兩個服務互打）。
    if (!from || !to || from === to) continue
    const key = `${from}\u0000${to}`
    const pair = pairs.get(key) ?? { from, to, of: [] }
    pair.of.push(r.id)
    if (!pairs.has(key)) pairs.set(key, pair)
  }

  const people = new Set((logical.people ?? []).map((p) => p.id))
  const links: Link[] = [...pairs.values()].map((pair) => ({
    // 剛好一條契約時才綁得上——這條線就是那一條契約，弱檢查認得出來。
    // 收攏過的線沒有對應的模型元素，硬指一條等於說謊。
    connection: pair.of.length === 1 ? pair.of[0]! : '',
    from: pair.from,
    to: pair.to,
    fromPerson: people.has(pair.from),
    kind: 'primary',
    purpose: purposeOf(project, pair.of),
  }))

  // ── 框：畫得到線的人，加上這個環境有的系統 ──────────────
  const used = new Set(links.filter((l) => l.fromPerson).map((l) => l.from))
  const shapes: Shape[] = [
    ...(logical.people ?? [])
      .filter((p) => used.has(p.id))
      .map((p) => ({
        id: p.id,
        loomId: p.id,
        kind: 'person',
        label: p.slug,
        style: STYLE.person,
      })),
    ...(logical.systems ?? [])
      .filter((x) => present.has(x.id))
      .map((x) => ({
        id: x.id,
        loomId: x.id,
        kind: 'softwareSystem',
        label: x.slug,
        // 副標放全名而不是位址：這張圖是給不熟這套系統的人看的，
        // `bmy` 四個字對他沒有意義。位址是部署圖的事。
        detail: x.name === x.slug ? undefined : x.name,
        style: x.external ? STYLE.externalSystem : STYLE.ownedSystem,
      })),
  ]

  return { shapes, links }
}

/**
 * 收攏過的那條線上該寫什麼。
 *
 * 剛好一條就寫它的用途。多條的話**寫數量，不是把用途串起來**——
 * 串起來的字會長到擋住半張圖，而且讀的人會以為那是一條連線的完整說明。
 * 想看是哪幾條，部署圖上一條一條都在。
 */
function purposeOf(project: Project, relationships: string[]): string {
  if (relationships.length === 1) {
    const one = (project.logical.relationships ?? []).find((r) => r.id === relationships[0])
    return one?.purpose ?? ''
  }
  return `${relationships.length} 條契約`
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
  const shapes = canvasShapes(project, environment, links)

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
      const id = l.connection ? `${l.connection}:${l.from}:${l.to}` : `${l.from}:${l.to}`
      const style = l.kind === 'fallback' ? EDGE.fallback : EDGE.primary
      return [
        // `connection` 是空的＝這條線不代表單一模型元素（context 圖收攏過的
        // 那種）。硬指其中一條契約等於說謊，所以乾脆不給 `loomId`。
        `        <object label="${esc(l.purpose)}"${l.connection ? ` loomId="${esc(l.connection)}"` : ''} loomKind="connection" id="${esc(id)}">`,
        `          <mxCell style="${style}" edge="1" parent="1" source="${esc(l.from)}" target="${esc(l.to)}">`,
        `            <mxGeometry relative="1" as="geometry"/>`,
        `          </mxCell>`,
        `        </object>`,
      ].join('\n')
    })
    .join('\n')

  return mxfile(environment.id, environment.slug, [cells, edges])
}

/**
 * 一張空白的圖。
 *
 * 使用者自己建的圖從這裡開始——他要自己畫，或者把畫好的貼進來，
 * 然後用「標註」把形狀接上模型。
 *
 * ⚠️ **那兩個 root cell 少了 draw.io 會讀不進去**，而症狀是編輯器一片空白，
 * 跟「這張圖本來就是空的」長得一模一樣。
 */
export function blankXml(id: string, name: string): string {
  return mxfile(id, name, [])
}

/** `.drawio` 的外殼。只有這裡知道那兩個 root cell 非有不可。 */
function mxfile(id: string, name: string, body: string[]): string {
  return [
    '<mxfile host="diagram-loom">',
    `  <diagram id="${esc(id)}" name="${esc(name)}">`,
    '    <mxGraphModel dx="0" dy="0" grid="1" gridSize="10" page="1" pageWidth="1169" pageHeight="827">',
    '      <root>',
    '        <mxCell id="0"/>',
    '        <mxCell id="1" parent="0"/>',
    ...body,
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

/**
 * 拿到一個形狀的「擁有者」：帶著 id 與自訂屬性的那一個元素。
 *
 * draw.io 的形狀有兩種寫法：光禿禿的 `<mxCell id=... value=...>`，
 * 或者被 `<object label=... id=...>` 包起來（要帶自訂屬性時就會變成這樣，
 * `<UserObject>` 也是同一回事）。**只有後者放得下 `loomId`。**
 *
 * 認的是「父元素有沒有 id」而不是標籤名字：`<root>` 沒有 id，
 * 而包裝元素一定有——這樣就不必去記 draw.io 用過幾種包裝的名字。
 */
function ownerOf(cell: Element): Element {
  const parent = cell.parentElement
  return parent?.hasAttribute('id') ? parent : cell
}

/** 圖上所有形狀（含線）。root 的那兩個 cell 不算。 */
function allCells(doc: Document): Element[] {
  return Array.from(doc.querySelectorAll('mxCell')).filter(
    (c) => c.getAttribute('vertex') === '1' || c.getAttribute('edge') === '1',
  )
}

/**
 * 把 draw.io 的標籤變成一行純文字。
 *
 * 標籤可以是 HTML（`html=1` 是預設），所以使用者畫的圖上常常是
 * `<b>Apache</b><br>叢集`。直接拿去比對名字會一個都對不上。
 */
function plain(label: string): string {
  return label
    .replace(/<br\s*\/?>|<\/(div|p|li)>/gi, '\n')
    .replace(/<[^>]*>/g, '')
    .replace(/&nbsp;/g, ' ')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    // `&amp;` 最後才解，否則 `&amp;lt;` 會被解兩次變成 `<`。
    .replace(/&amp;/g, '&')
    .split('\n')[0]!
    .trim()
}

/**
 * 圖上**還沒指定**代表誰的形狀。
 *
 * 這是 [`boundShapes`] 的反面，而反面才是使用者上傳自己畫的圖時看到的東西：
 * 整張圖沒有一個形狀綁著模型。
 *
 * # 沒有文字的也要列
 *
 * 空白的方框多半是裝飾，列出來很吵。但**這個工具的命是怕漏**——自己決定
 * 「這個不用管」，使用者就永遠不知道有這回事了。所以照樣送出來，
 * 由畫面決定要不要摺起來，而「還剩幾個」那個數字仍然是誠實的。
 *
 * # 我們自己畫的不算
 *
 * 帶 `loomKind` 的形狀是我們產的（例如「人」，它刻意沒有 `loomId`）。
 * 那些不是「還沒指定」，是「本來就不該指定」。
 */
export function unboundShapes(xml: string): UnboundShape[] {
  const doc = new DOMParser().parseFromString(xml, 'text/xml')

  return allCells(doc).flatMap((cell) => {
    const owner = ownerOf(cell)
    if (owner.hasAttribute('loomId') || owner.hasAttribute('loomKind')) return []

    const id = owner.getAttribute('id')
    if (!id) return []

    // 包起來的時候文字在 `label`，沒包的時候在 `value`。
    const label = owner === cell ? cell.getAttribute('value') : owner.getAttribute('label')
    return [{ cell: id, label: plain(label ?? ''), edge: cell.getAttribute('edge') === '1' }]
  })
}

/**
 * 把一個形狀指給某個模型元素。
 *
 * # 為什麼要動 XML 的結構
 *
 * `loomId` 是自訂屬性，而**自訂屬性只放得進 `<object>`**。使用者自己畫的
 * 方框是光禿禿的 `<mxCell>`，所以指定的動作實際上是「把它包起來」：
 * `value` 變成 `label`、`id` 搬到外層。draw.io 自己按「編輯資料」時做的
 * 也是這件事。
 *
 * 改的是**編輯器剛剛交出來的那份 XML**，不是從模型重產一張——
 * 重產會洗掉使用者排好的版面，而這裡動的每一個形狀都是他自己畫的。
 */
export function bind(xml: string, cell: string, loomId: string): string {
  const doc = new DOMParser().parseFromString(xml, 'text/xml')
  const target = elementWithId(doc, cell)
  if (!target) return xml

  if (target.tagName === 'mxCell') {
    // ⚠️ 一定要走 `NS` 這一對。`createElement` / `setAttribute` 走的是 HTML 的
    // 規矩：屬性名會被轉成小寫（`loomId` → `loomid`），而元素還會被塞上
    // XHTML 的 namespace。兩個都足以讓 draw.io 與我們自己的 `[loomId]`
    // 查詢通通看不見這個綁定——而且失敗得很安靜。
    const wrapper = doc.createElementNS(null, 'object')
    wrapper.setAttributeNS(null, 'label', target.getAttribute('value') ?? '')
    wrapper.setAttributeNS(null, 'loomId', loomId)
    wrapper.setAttributeNS(null, 'id', cell)
    // 包起來之後 `id` 與 `value` 就歸外層管。留在裡面的話 draw.io 讀進去
    // 會有兩個 id，而它認的是外面那個——留著只是等著哪天對不上。
    target.removeAttribute('id')
    target.removeAttribute('value')
    target.parentNode?.replaceChild(wrapper, target)
    wrapper.appendChild(target)
  } else {
    // 同上：`setAttribute` 會把名字轉成小寫。
    target.setAttributeNS(null, 'loomId', loomId)
  }

  return new XMLSerializer().serializeToString(doc)
}

/**
 * 取消指定。
 *
 * 指錯了一定要收得回來：綁定之後對帳就會把它當事實，而錯的那一項
 * 不會有人再檢查它。
 *
 * 包裝元素若是我們剛剛才加上去的（只剩 `label` 與 `id`），就拆回原本的
 * `<mxCell>`。使用者自己加過資料的 `<object>` 則原樣留著——那是他的東西。
 */
export function unbind(xml: string, cell: string): string {
  const doc = new DOMParser().parseFromString(xml, 'text/xml')
  const target = elementWithId(doc, cell)
  if (!target || !target.hasAttribute('loomId')) return xml

  target.removeAttribute('loomId')

  const inner = target.querySelector('mxCell')
  const bare = target.getAttributeNames().every((n) => n === 'label' || n === 'id')
  if (inner && bare && !target.hasAttribute('loomKind')) {
    inner.setAttribute('id', cell)
    const label = target.getAttribute('label')
    if (label) inner.setAttribute('value', label)
    target.parentNode?.replaceChild(inner, target)
  }

  return new XMLSerializer().serializeToString(doc)
}

/** 依 id 找元素。不用 `querySelector`：id 裡可能有引號，選擇器會被它咬到。 */
function elementWithId(doc: Document, id: string): Element | null {
  return Array.from(doc.querySelectorAll('[id]')).find((e) => e.getAttribute('id') === id) ?? null
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
  return paint(xml, (key) => lit.has(key), false)
}

/**
 * 只讓一個形狀亮著，其他全部調暗——包括使用者自己畫的。
 *
 * 標註清單用它回答「這一項到底是圖上哪一個框」。清單有 47 個名字時，
 * 光靠名字對不出來，而**對不出來的人就會亂指**——那比沒有這個功能更糟。
 *
 * 這裡刻意連沒有 `loomKind` 的形狀也調暗（[`dim`] 不會）：要指定的東西
 * 正是使用者自己畫的那些，不碰它們就等於整張圖都亮著，等於沒指。
 */
export function spotlight(xml: string, cell: string): string {
  return paint(xml, (key) => key === cell, true)
}

/**
 * 把我們塗上去的螢光筆擦掉。
 *
 * 少了它，關掉篩選之後圖還是暗的——`opacity` 已經寫進編輯器交回來的 XML 了，
 * 而下一次重塗看到「沒有在篩選」就原封不動送回去。
 */
export function undim(xml: string): string {
  return paint(xml, () => true, true)
}

/**
 * 螢光筆的本體。
 *
 * `unbound` 決定碰不碰使用者自己畫的形狀：篩選只講模型的事，所以不碰；
 * 標註要指的正是那些形狀，所以碰。
 */
function paint(xml: string, isLit: (key: string) => boolean, unbound: boolean): string {
  const doc = new DOMParser().parseFromString(xml, 'text/xml')

  for (const cell of allCells(doc)) {
    const owner = ownerOf(cell)
    const ours = owner.hasAttribute('loomKind') || owner.hasAttribute('loomId')
    if (!ours && !unbound) continue

    // 線的 `loomId` 是連線 id，形狀的是元素 id；人沒有 `loomId`，
    // 用 cell id 認（`Highlight.shapes` 裡放的就是人的 id）。
    const key = owner.getAttribute('loomId') ?? owner.getAttribute('id') ?? ''
    cell.setAttribute('style', withOpacity(cell.getAttribute('style') ?? '', isLit(key)))
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
