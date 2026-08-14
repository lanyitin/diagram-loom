/**
 * 探針：把 `docs/canvas-engine.md` 那四個問題裡**不必開視窗就答得出來的**答掉。
 *
 * | 問題 | 這裡答不答得了 |
 * | --- | --- |
 * | 1 elkjs 自己接，巢狀排得好嗎 | ✅ 座標是純計算，客觀檢查 |
 * | 2 少了 draw.io 的編輯器多痛 | ❌ 要人去用（見 `pnpm app`） |
 * | 3 點形狀標註做起來多快 | ❌ 同上 |
 * | 4 stencils 搬不搬得動 | ✅ 解析與註冊都在記憶體裡 |
 *
 * 「排得好不好看」沒辦法自動判斷，但「**壞掉**」有客觀定義。四項檢查
 * 照抄 `docs/nested-layout.md`，這樣數字才跟 draw.io 那一輪比得起來。
 *
 * 離開碼：0 全過、1 有項目失敗。
 */

import { readFileSync } from 'node:fs'
import { Window } from 'happy-dom'

import { countNodes, crowd, fixture } from './fixture.mjs'
import { find, layout, walk } from './layout.mjs'

const results = []

function check(name, ok, detail = '') {
  results.push({ name, ok, detail })
  console.log(`${ok ? '  ✓' : '  ✗'} ${name}${detail ? `　${detail}` : ''}`)
}

function section(title) {
  console.log(`\n── ${title} ${'─'.repeat(Math.max(0, 56 - title.length))}`)
}

const round = (n) => Math.round(n * 10) / 10

// ── 問題一：巢狀排版 ────────────────────────────────────────────
//
// 四項客觀檢查，跟 draw.io 那一輪同一組：
//   1 子節點跑到父框外面　2 兄弟互相重疊　3 父子關係被改掉　4 loomId 掉了

section('問題一：elkjs 自己接，巢狀排得好嗎')

const model = fixture()
const { laid, ms } = await layout(model)

/** 子節點在父框內嗎。ELK 回的座標是相對於父節點的，所以直接跟寬高比。 */
const escaped = []
walk(laid, (node, parent) => {
  if (parent.id === 'root') return
  const out =
    node.x < 0
    || node.y < 0
    || node.x + node.width > parent.width + 0.5
    || node.y + node.height > parent.height + 0.5
  if (out) {
    escaped.push(
      `${node.id} 跑出 ${parent.id}（子 ${round(node.x)},${round(node.y)} `
      + `${round(node.width)}×${round(node.height)}｜父 ${round(parent.width)}×${round(parent.height)}）`,
    )
  }
})
check('子節點都在父框內', escaped.length === 0, escaped[0] ?? '')

/** 同一個父節點底下的兄弟不重疊。 */
const overlaps = []
const byParent = new Map()
walk(laid, (node, parent) => {
  if (!byParent.has(parent.id)) byParent.set(parent.id, [])
  byParent.get(parent.id).push(node)
})
for (const [parentId, siblings] of byParent) {
  for (let i = 0; i < siblings.length; i += 1) {
    for (let j = i + 1; j < siblings.length; j += 1) {
      const [a, b] = [siblings[i], siblings[j]]
      const hit =
        a.x < b.x + b.width
        && b.x < a.x + a.width
        && a.y < b.y + b.height
        && b.y < a.y + a.height
      if (hit) overlaps.push(`${a.id} 疊到 ${b.id}（在 ${parentId} 裡）`)
    }
  }
}
check('兄弟節點沒有互相重疊', overlaps.length === 0, overlaps[0] ?? '')

/** 父子關係沒被改掉。 */
const wanted = new Map()
const record = (nodes, parent) => {
  for (const node of nodes) {
    wanted.set(node.id, parent)
    record(node.children ?? [], node.id)
  }
}
record(model.nodes, 'root')
const moved = []
walk(laid, (node, parent) => {
  if (wanted.get(node.id) !== parent.id) moved.push(`${node.id}：${wanted.get(node.id)} → ${parent.id}`)
})
check('父子關係沒被改掉', moved.length === 0, moved[0] ?? '')

/**
 * 綁定活得下來嗎。
 *
 * elkjs 只認 `id`，其他欄位它會**原封不動地留在物件上**——因為進去的
 * 就是我們自己的物件。draw.io 那邊靠的是 `<object loomId=...>` 活過
 * 一次 XML 往返，這裡連往返都沒有。
 */
const ids = []
walk(laid, (node) => ids.push(node.id))
check(
  'loomId 一個都沒掉',
  ids.length === countNodes(model.nodes),
  `${ids.length} / ${countNodes(model.nodes)} 個節點`,
)

/** 容器有沒有縮到剛好包住子節點（draw.io 那輪 layered 是 205×444）。 */
const tpe = find(laid, 'site-tpe')
check(
  '容器縮到剛好包住子節點',
  tpe.width < 700,
  `台北機房 ${round(tpe.width)}×${round(tpe.height)}（素材原始值 700×200）`,
)

/** 邊有沒有走直角。轉彎點數是 `docs/nested-layout.md` 用的同一個指標。 */
const bends = (laid.edges ?? []).reduce(
  (n, e) => n + (e.sections ?? []).reduce((m, s) => m + (s.bendPoints?.length ?? 0), 0),
  0,
)
check('邊走直角（有轉彎點）', bends > 0, `${bends} 個轉彎點`)

console.log(`  · 整張圖 ${round(laid.width)}×${round(laid.height)}，排版 ${round(ms)} ms`)

// ── 額外：draw.io 做不到的兩件事 ────────────────────────────────
//
// L4「釘不住節點」與 L6「不能只重排一個容器」，`docs/nested-layout.md` 記的是
// **draw.io 那個 applier** 的限制。自己接就沒有那個 applier 了——所以要驗，
// 而其中一個的答案跟當初的猜測相反。

section('額外：draw.io 的 L4 / L6 自己接會不會消失')

const pinned = await layout(model, { fixed: { f5: { x: 800, y: 120 } } })
const f5 = find(pinned.laid, 'f5')
// ⚠️ 這一項是**確認它做不到**。換了引擎一樣釘不住，所以它不是 draw.io 的錯，
// 是 ELK 的：`elk.position` 要 INTERACTIVE 那組策略才會被看，而下一項證明
// 那組策略跟 INCLUDE_CHILDREN（巢狀的命脈）根本併不了。
check(
  'L4：elk.position 一樣釘不住（換引擎也救不了）',
  f5.x !== 800 || f5.y !== 120,
  `F5 落在 ${round(f5.x)},${round(f5.y)}，要求的是 800,120`,
)

let interactiveError = ''
try {
  await layout(model, {
    fixed: { f5: { x: 800, y: 120 } },
    options: {
      'elk.interactive': 'true',
      'elk.layered.cycleBreaking.strategy': 'INTERACTIVE',
      'elk.layered.layering.strategy': 'INTERACTIVE',
      'elk.layered.crossingMinimization.strategy': 'INTERACTIVE',
      'elk.layered.nodePlacement.strategy': 'INTERACTIVE',
    },
  })
} catch (e) {
  interactiveError = e.message
}
check(
  'L4：INTERACTIVE 策略跟巢狀併不了，而且 ELK 自己會說',
  interactiveError.includes('hierarchy aware processor'),
  interactiveError.slice(0, 96) || '這次沒有拋錯——結論要重寫',
)

/** L6：只排一個容器，別人不准動。自己接的話，把子樹單獨送進去就好。 */
const before = find(laid, 'site-bak')
const subtree = {
  nodes: model.nodes.filter((n) => n.id === 'site-tpe'),
  edges: [],
}
const partial = await layout(subtree)
check(
  'L6：只重排一個容器做得到（draw.io 做不到）',
  find(partial.laid, 'site-bak') === null && find(partial.laid, 'site-tpe') !== null,
  `備援機房沒有進到這次排版（原本 ${round(before.width)}×${round(before.height)}）`,
)

// ── 規模 ────────────────────────────────────────────────────────
//
// draw.io 那邊實測 2480 個節點 6 秒。自己接的 elkjs 要在同一個量級。

section('規模：自己接的 elkjs 跑得動嗎')

for (const [machines, per] of [[20, 5], [60, 8], [120, 12]]) {
  const big = crowd(machines, per)
  const total = countNodes(big.nodes)
  const { ms: took } = await layout(big)
  check(`${total} 個節點`, took < 15000, `${round(took)} ms`)
}

// ── 問題四：形狀庫 ──────────────────────────────────────────────
//
// 「幾乎每一張別人畫好的圖，打開都是一堆空白方框」是換引擎最大的風險。
// 大宗（AWS / Azure / GCP / Cisco / Rack）都在 stencils/*.xml 那半邊，
// 而那是**資料**不是程式——所以問題只有一個：maxGraph 讀不讀得懂同一份 XML。

section('問題四：draw.io 的 stencils/*.xml 搬不搬得動')

const window = new Window()
globalThis.DOMParser = window.DOMParser
globalThis.document = window.document
globalThis.window = window

const { StencilShape, StencilShapeRegistry } = await import('@maxgraph/core')

check(
  'maxGraph 還留著 stencil 機制',
  typeof StencilShape === 'function' && typeof StencilShapeRegistry?.add === 'function',
  'StencilShape / StencilShapeRegistry',
)

const VENDOR = new URL('../../../vendor/drawio/stencils/', import.meta.url).pathname
// 挑的是最大的那幾包與最常見的雲廠商——真實世界的架構圖幾乎都在用它們。
const FILES = ['arrows.xml', 'aws4.xml', 'azure.xml', 'gcp3.xml', 'kubernetes.xml', 'cisco19.xml', 'networks.xml', 'rack/general.xml']

let loaded = 0
let failed = 0
for (const file of FILES) {
  let xml
  try {
    xml = readFileSync(VENDOR + file, 'utf8')
  } catch {
    check(`${file}`, false, '讀不到檔案（vendor/drawio 沒放進來？）')
    continue
  }

  const started = performance.now()
  const doc = new window.DOMParser().parseFromString(xml, 'text/xml')
  const shapes = [...doc.documentElement.children].filter((el) => el.tagName === 'shape')

  let ok = 0
  let bad = ''
  for (const el of shapes) {
    try {
      const stencil = new StencilShape(el)
      // 解析成功的判準：名字、長寬、以及至少一段路徑（背景或前景）。
      // 只 new 出來不算——建構子吞掉錯誤的話會得到一個空殼。
      if (stencil.w0 > 0 && stencil.h0 > 0 && (stencil.bgNode || stencil.fgNode)) ok += 1
      StencilShapeRegistry.add(el.getAttribute('name'), stencil)
    } catch (e) {
      if (!bad) bad = e.message
    }
  }
  loaded += ok
  failed += shapes.length - ok
  check(
    `${file}`,
    ok === shapes.length && shapes.length > 0,
    `${ok} / ${shapes.length} 個形狀，${round(performance.now() - started)} ms`
      + (bad ? `｜${bad}` : ''),
  )
}

check('形狀庫整體', failed === 0, `讀進 ${loaded} 個形狀，失敗 ${failed} 個`)

/** 註冊完之後真的查得到——不然「讀進去了」只是解析成功而已。 */
const sample = StencilShapeRegistry.get('mxgraph.arrows.arrow_down')
  ?? StencilShapeRegistry.get('Arrow Down')
check('註冊之後查得回來', Boolean(sample), sample ? `w0=${sample.w0} h0=${sample.h0}` : '查不到')

// ── 總結 ────────────────────────────────────────────────────────

const failedChecks = results.filter((r) => !r.ok)
console.log(`\n${'═'.repeat(60)}`)
console.log(`${results.length - failedChecks.length} / ${results.length} 通過`)
for (const r of failedChecks) console.log(`  ✗ ${r.name}　${r.detail}`)
process.exit(failedChecks.length === 0 ? 0 : 1)
