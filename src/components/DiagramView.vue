<script setup lang="ts">
/**
 * 圖：組合根。
 *
 * # 這裡以前是一個 iframe
 *
 * 換成自己跑 maxGraph 之後，**整套 `postMessage` 協定不見了**——沒有
 * `autosave` 事件、沒有「等排版跑完才能縮放」的計時器與逾時、沒有
 * `zoom` 穿透 iframe 的抵銷。那些全是隔著一層 iframe 才有的問題
 * （見 `docs/maxgraph-findings.md`）。
 *
 * # 判斷都不在這個檔案裡
 *
 * 排版在 `graph/layout.ts`、建 cell 在 `graph/render.ts`、綁定在
 * `graph/binding.ts`、螢光筆在 `graph/highlight.ts`、`.drawio` 讀寫在
 * `graph/mxml.ts`——每一支都不需要畫面，所以都在秒級測試裡守著。
 * 這裡只負責把它們接起來，以及跟 Rust 要東西。
 *
 * # 跟 Rust 的分界沒有變
 *
 * 「勾了這條契約誰該亮」問 `diagram_focus`、「萬用字元展開成哪幾台」問
 * `diagram_links`、「這個框可以指給誰」問 `diagram_targets`、
 * 「存不存得下去」問 `diagram_save`。換畫布引擎不該動到這條線。
 */
import { computed, ref, watch } from 'vue'
import type { Cell } from '@maxgraph/core'

import { commands } from '../lib/bindings'
import { useProject } from '../lib/store'
import { blankXml, canvasShapes, contextDrawing } from '../lib/diagram'
import { bind, boundShapes, shapeLabels, unboundShapes } from '../lib/graph/binding'
import { fold } from '../lib/graph/fold'
import { matching, step } from '../lib/graph/find'
import { dim, smeared, spotlight, undim } from '../lib/graph/highlight'
import { clipboard, contextMenu, keyboard } from '../lib/graph/interact'
import type { Palette } from '../lib/graph/shapes'
import { parseStyle } from '../lib/graph/mxml'
import DiagramBinder from './DiagramBinder.vue'
import DiagramCanvas from './DiagramCanvas.vue'
import DiagramFilter from './DiagramFilter.vue'
import FormatPanel from './FormatPanel.vue'
import MetadataDialog from './MetadataDialog.vue'
import ShapePicker from './ShapePicker.vue'
import type { Annotation, Contract, DiagramInfo, DiagramKind, Highlight, Link } from '../lib/model'

const store = useProject()

const canvas = ref<InstanceType<typeof DiagramCanvas> | null>(null)
const graph = computed(() => canvas.value?.graph ?? null)
const selection = ref<Cell[]>([])
const status = ref('')
const links = ref<Link[]>([])
/** 正在跟 Rust 要線。按著「重畫」不放不該送出第二趟。 */
const redrawing = ref(false)

/** 存過的圖讀進來的 XML。`null` 表示「從模型產一張」。 */
const xml = ref<string | null>(null)

const environmentChosen = ref<string | null>(null)
const environment = computed(
  () =>
    store.environments.find((e) => e.id === environmentChosen.value)
    ?? store.visibleEnvironments[0]
    ?? store.environments[0]
    ?? null,
)

/**
 * 這張圖要畫的形狀與線。
 *
 * # 為什麼形狀跟線綁在一起算
 *
 * 因為**換一張自動圖等於換一份線**：部署圖的線是這個環境展開後的連線
 * （Rust 算的，一條萬用字元契約長成 N×M 條），context 圖的線是收攏到
 * 系統層級的契約。兩者分開算的話，切到 context 圖的那一瞬間會出現
 * 「context 的框配部署圖的線」——線的兩端都對不上，整批被丟掉，
 * 而畫面上看起來只是「線不見了」。
 *
 * 部署圖那一份**要帶 `links`**：人住在邏輯層，只有連線說得出「這個環境
 * 用到了誰」。少了人，起點是人的線就會被丟掉，而丟掉一條線會讓後面所有
 * 線的轉彎點錯開（見 `lib/diagram.ts` 的 `canvasShapes`）。
 */
const full = computed(() => {
  const env = environment.value
  if (!env || !store.snapshot) return { shapes: [], links: [] }
  if (showing.value === context.value) return contextDrawing(store.snapshot.project, env)
  return {
    shapes: canvasShapes(store.snapshot.project, env, links.value),
    links: links.value,
  }
})

/**
 * 收起來的那幾個框。**這是檢視狀態，不是模型的一部分。**
 *
 * 換一張圖或換一個環境就歸零：id 只在同一張圖裡有意義，留著會讓下一張圖
 * 莫名其妙少了幾個框，而且沒有任何線索說得出為什麼。
 */
const collapsed = ref(new Set<string>())

/** 套完折疊之後，真正要畫的東西。折疊怎麼做見 `graph/fold.ts`。 */
const drawing = computed(() => fold(full.value.shapes, full.value.links, collapsed.value))
const shapes = computed(() => drawing.value.shapes)
const drawn = computed(() => drawing.value.links)

/** 收起來藏了幾個東西。**一定要講出來**，理由見 `graph/fold.ts`。 */
const hiddenCount = computed(() => full.value.shapes.length - shapes.value.length)

/**
 * 使用者點了 ± 。
 *
 * 存過的圖不給折疊（畫布那邊 `foldable` 是空的），所以這裡只會被自動產生
 * 的那兩張圖叫到——它們的座標本來就是我們算的，重排不會洗掉任何人的東西。
 */
function onFold(ids: string[], collapse: boolean) {
  const next = new Set(collapsed.value)
  for (const id of ids) {
    if (collapse) next.add(id)
    else next.delete(id)
  }
  collapsed.value = next
}

/**
 * 這個環境一台機器都沒有。
 *
 * 空的部署圖跟壞掉的編輯器長得一模一樣——都是一片空白格線。
 * 使用者第一個念頭會是「工具壞了」，而不是「這裡還沒填東西」。
 */
const empty = computed(() => shapes.value.length === 0 && !xml.value)

/** 一條萬用字元連線是 N×M 條線——12 台連 12 台就是 144 條。 */
const TOO_MANY = 600
const tooMany = computed(() => drawn.value.length > TOO_MANY)

// ── 存檔 ────────────────────────────────────────────────────────

const diagrams = ref<DiagramInfo[]>([])

/**
 * App 自動產生的那兩張圖叫什麼。**名字從 Rust 拿**（`diagram_catalog`）——
 * 在這裡自己抄一份的話就有兩個「保留字」，而改名的那天只會有一邊跟著改。
 *
 * `deployment` 是部署圖（機器、服務實體、位址），`context` 是 context 圖
 * （人與系統，一個系統一個框）。
 */
const deployment = ref('diagram-loom-deployment')
const context = ref('diagram-loom-context')
const generated = computed(() => [deployment.value, context.value])
const isGenerated = (name: string) => generated.value.includes(name)

const opened = ref<string | null>(null)
const drawnFrom = ref('')
const modelNow = ref('')
const unsaved = ref(false)
const newName = ref<string | null>(null)

const showing = computed(() => opened.value ?? deployment.value)

/**
 * 這張圖是詳圖還是簡圖。
 *
 * 還沒進目錄檔（剛產出來、還沒存過）時由名字決定，跟 Rust 的 `kind_of`
 * 同一個答案：部署圖是詳圖，context 圖是簡圖（它的線收攏過，一條線代表
 * 一群契約，撐不住「一個 `loomId` ↔ 一個元素」）。
 *
 * ⚠️ 這裡**寧可猜簡圖**：猜成詳圖會讓一張其實不對帳的圖標著「詳圖」，
 * 而「說有檢查、其實沒有」正是這個專案最怕的那種錯。
 */
const kind = computed<DiagramKind>(
  () =>
    diagrams.value.find((d) => d.name === showing.value)?.kind
    ?? (showing.value === deployment.value ? 'detail' : 'simple'),
)

/**
 * 換一張圖是詳圖還是簡圖。
 *
 * # 為什麼這是使用者的判斷，不是程式猜的
 *
 * 同一張手繪圖，他可能忠實畫了每一台，也可能把 12 台收成一個框。
 * 猜錯的兩個方向都很糟：當成詳圖會噴出一整頁他沒有畫錯的差異，
 * 當成簡圖則是**安靜地不檢查**——而後者正是這個專案最怕的那種失敗。
 */
async function setKind(next: DiagramKind) {
  const env = environment.value
  if (!env) return
  const res = await commands.diagramSetKind(env.id, showing.value, next)
  if (res.status !== 'ok') return failed(res.error)
  await refreshCatalog()
  // 選單的內容跟著種類變——簡圖才給邏輯層。
  await refreshTargets()
}
const onDisk = computed(() => diagrams.value.some((d) => d.name === showing.value))
/** 這張圖畫的是舊的模型。存檔會被 Rust 擋下來，所以先講。 */
const stale = computed(() => Boolean(drawnFrom.value) && drawnFrom.value !== modelNow.value)

// ── 篩選與標註 ──────────────────────────────────────────────────

const picked = ref<string[]>([])
const contracts = ref<Contract[]>([])
const highlight = ref<Highlight | null>(null)
const totalShapes = ref(0)
const filterOpen = ref(false)
const filtering = computed(() => picked.value.length > 0 || store.onlyProblems)
const litCount = computed(() => highlight.value?.shapes.length ?? 0)

const binderOpen = ref(false)
const pickerOpen = ref(false)
const spotted = ref<string | null>(null)
const assigned = ref<{ cell: string; label: string; target: string }[]>([])
const annotation = ref<Annotation>({ shapes: [], connections: [], guesses: [] })
const editing = ref<Cell | null>(null)

/** 圖上還沒指定的形狀。**每次回頭問圖**，不留自己的副本。 */
const unbound = ref<ReturnType<typeof unboundShapes>>([])

function refreshUnbound() {
  const g = graph.value
  unbound.value = g ? unboundShapes(g.getDataModel()) : []
}

// ── 搜尋 ────────────────────────────────────────────────────────

/**
 * 在圖上找形狀，找到就把它移到畫面中間並標亮。
 *
 * # 為什麼圖需要自己的搜尋框，不能沿用工具列那一個
 *
 * 工具列那個 `store.search` 是**篩選**：表格會少幾列。圖上不能少東西——
 * 一張圖的意義有一半在「誰跟誰連著」，藏掉幾個框會讓剩下的線指向空氣。
 *
 * 所以圖上的搜尋是**帶你過去**，不是把別的藏起來。這也是為什麼它不改
 * `store.search`：那個一改，另一頁的表格會跟著被篩掉，而使用者根本
 * 沒在看那一頁。
 *
 * # 一張圖可能比視窗大很多
 *
 * 幾百個形狀的圖縮到看得見全部時，字已經小到讀不出來；放大到讀得出來時
 * 又只看得到一小塊。「我知道有這台機器，但它在哪」是這張圖最常見的問題，
 * 而在這之前唯一的辦法是用滑鼠拖著找。
 */
const needle = ref('')
const at = ref(0)

/** 圖上每一個形狀的文字。每次回頭問圖，理由同 `unbound`。 */
const labels = ref<ReturnType<typeof shapeLabels>>([])

function refreshLabels() {
  const g = graph.value
  labels.value = g ? shapeLabels(g.getDataModel()) : []
}

const hits = computed(() => matching(labels.value, needle.value))

/** 走到第 n 個並帶過去。`hits` 變了也走這裡，所以「跳過去」只有一條路。 */
function goTo(index: number) {
  at.value = step(index, hits.value.length, 0)
  const target = hits.value[at.value]
  // 找不到就把標亮清掉。留著上一次的話，畫面會說「這個」而搜尋框說「沒有」。
  if (!target) return spot(null)
  spot(target.cell)
  canvas.value?.center(target.cell)
}

/** 打字就跳到第一個。等使用者按 Enter 才動的話，他會以為沒反應。 */
watch(needle, () => goTo(0))

function stepBy(delta: number) {
  if (hits.value.length === 0) return
  goTo(step(at.value, hits.value.length, delta))
}

/** 收起搜尋。**標亮要一起收掉**，否則圖上會留一個沒有人解釋得了的亮框。 */
function clearSearch() {
  needle.value = ''
  spot(null)
}

const failed = (error: unknown) => {
  store.error = (error as { message?: string })?.message ?? String(error)
}

// ── 圖的清單與讀寫 ──────────────────────────────────────────────

async function refreshCatalog() {
  const env = environment.value
  if (!env) return
  const res = await commands.diagramCatalog(env.id)
  if (res.status !== 'ok') return failed(res.error)
  diagrams.value = res.data.diagrams
  modelNow.value = res.data.model
  deployment.value = res.data.deployment
  context.value = res.data.context
}

/** 打開這個環境該顯示的那張圖。 */
async function reopen() {
  await refreshCatalog()
  await openDiagram(showing.value)
}

/**
 * 打開某一張圖：存過的就讀回來，還沒存過的那兩張自動圖就從模型產。
 *
 * 自動產生的那兩張在清單裡永遠看得到，即使一次都還沒存過——它們是這個
 * 環境的主圖，從清單消失只會讓人以為不見了。
 */
async function openDiagram(name: string) {
  if (diagrams.value.some((d) => d.name === name)) await readDiagram(name)
  else if (isGenerated(name)) await regenerate(name)
  else store.error = `找不到圖 ${name}`
}

async function readDiagram(name: string) {
  const env = environment.value
  if (!env) return
  const res = await commands.diagramRead(env.id, name)
  if (res.status !== 'ok') return failed(res.error)

  opened.value = name
  drawnFrom.value = res.data.model
  // 存過的圖帶著使用者排好的座標，所以**不重新排版**——排下去等於把他的版面洗掉。
  xml.value = res.data.xml
  reset()
}

/**
 * 從模型重產一張自動圖。
 *
 * ⚠️ 這會蓋掉使用者在這張圖上排的版面。所以只有兩種情況會走到這裡：
 * 使用者自己按「重畫」，或者這張圖還沒存過（沒有版面可以蓋掉）。
 *
 * 預設重產**現在正在看的那一張**。看的是使用者自己畫的圖時退回部署圖——
 * 那是這個環境的主圖，也是「重畫」在只有一張自動圖的年代的意思。
 */
async function regenerate(name = isGenerated(showing.value) ? showing.value : deployment.value) {
  const env = environment.value
  if (!env || !store.snapshot || redrawing.value) return

  // 先把指紋更新到最新的。拿舊的當「這張圖畫的是哪一版」，使用者會卡在
  // 「重畫 → 存檔被擋 → 再重畫」的迴圈裡，而且畫面上完全看不出為什麼。
  await refreshCatalog()

  redrawing.value = true
  try {
    // 萬用字元展開成哪幾台是模型知識，所以由 Rust 算（`wiring.rs`）。
    // 在這裡自己比對一次 slug，就會養出第二套「什麼叫比對得上」。
    //
    // context 圖用不到它（它的線是收攏到系統層級的契約，不是展開後的連線），
    // 但還是要拿：切回部署圖時 `links` 要是新的，不然那張圖會用上一版模型
    // 的線去配新的框——兩端對不上就整批被丟掉。
    const res = await commands.diagramLinks(env.id)
    if (res.status !== 'ok') return failed(res.error)
    links.value = res.data
  } finally {
    redrawing.value = false
  }

  opened.value = name
  drawnFrom.value = modelNow.value
  xml.value = null
  reset()
  // shapes/links 沒變時 watch 不會觸發，所以直接叫畫布重畫。
  await canvas.value?.draw()
}

/** 換一張圖等於換一張畫布：上一張的標註、燈、未存檔全部歸零。 */
function reset() {
  assigned.value = []
  spotted.value = null
  unsaved.value = false
  selection.value = []
  // 折疊的 id 只在同一張圖裡有意義。留著會讓下一張圖莫名其妙少幾個框。
  collapsed.value = new Set()
}

/** 存檔。**模型變了 Rust 會擋下來**，這裡負責把理由講給使用者聽。 */
async function saveDiagram() {
  const env = environment.value
  const g = graph.value
  if (!env || !g) return

  // 螢光筆先擦掉，不然 `opacity` 會被存進檔案變成永久的。
  const wasSmeared = smeared(g.getDataModel())
  if (wasSmeared) canvas.value?.silently(() => undim(g.getDataModel()))

  const res = await commands.diagramSave(
    env.id,
    showing.value,
    canvas.value!.toXml({ id: env.id, name: showing.value }),
    drawnFrom.value,
  )

  if (wasSmeared) repaint()

  if (res.status !== 'ok') {
    failed(res.error)
    // 被擋下來多半是模型變了，順手把清單與指紋更新，畫面才會跟著說實話。
    await refreshCatalog()
    return
  }
  // 存完要接住新的指紋，不然手上那個就過期了，下一次存檔會被自己擋下來。
  drawnFrom.value = res.data
  unsaved.value = false
  status.value = `已存檔：${showing.value}`
  await refreshCatalog()
}

/** 使用者自己建一張新圖。從一張空白畫布開始，他自己畫或貼進來。 */
async function createDiagram(name: string) {
  const env = environment.value
  if (!env) return
  const res = await commands.diagramCreate(env.id, name, blankXml(env.id, name))
  if (res.status !== 'ok') return failed(res.error)
  newName.value = null
  await refreshCatalog()
  await readDiagram(res.data.name)
}

// ── 螢光筆 ──────────────────────────────────────────────────────

/**
 * 去問 Rust「誰該亮」，然後塗上去。
 *
 * 「勾了這條契約，哪些東西算相關」跟 lint 是同一類判斷，所以在 Rust
 * （`highlight.rs`）。在這裡自己算會養出第二套「什麼叫相關」。
 */
async function repaint() {
  const env = environment.value
  const g = graph.value
  if (!env || !g) return
  const model = g.getDataModel()

  // 每次都從**擦乾淨的**那份開始塗，否則上一次的 opacity 會疊在上面——
  // 關掉篩選之後圖還是暗的，而症狀看起來像「按了沒反應」。
  canvas.value?.silently(() => undim(model))

  // 標示某一個形狀時，螢光筆歸標註用。兩支筆同時塗，出來的顏色沒有人讀得懂。
  if (spotted.value) {
    canvas.value?.silently(() => spotlight(model, spotted.value!))
    return
  }

  const res = await commands.diagramFocus(env.id, {
    relationships: picked.value,
    problems: store.onlyProblems,
  })
  if (res.status !== 'ok') return failed(res.error)

  contracts.value = res.data.contracts
  highlight.value = res.data.highlight
  totalShapes.value = res.data.shapes

  // 什麼都沒勾就不塗——全部塗成「亮」等於把使用者自己調的半透明洗掉。
  if (!filtering.value) return
  const lit = new Set([...res.data.highlight.shapes, ...res.data.highlight.connections])
  canvas.value?.silently(() => dim(model, lit))
}

// ── 標註 ────────────────────────────────────────────────────────

/** 去問 Rust「這些沒指定的形狀可以指給誰」。 */
async function refreshTargets() {
  const env = environment.value
  const g = graph.value
  if (!binderOpen.value || !env || !g) return

  const res = await commands.diagramTargets(
    env.id,
    showing.value,
    boundShapes(g.getDataModel()).map((s) => s.id),
    unbound.value,
  )
  if (res.status !== 'ok') return failed(res.error)
  annotation.value = res.data
}

/** 指定一個形狀代表誰。改的是圖，不是模型——模型一個字都沒動。 */
function assign(cell: string, target: string) {
  const g = graph.value
  if (!g) return
  const shape = unbound.value.find((s) => s.cell === cell)
  bind(g.getDataModel(), cell, target)
  assigned.value = [...assigned.value, { cell, label: shape?.label ?? '', target }]
  // 指定完就把標示收掉：那個形狀已經從清單上消失了，燈還亮著只會讓人找不到自己在哪。
  if (spotted.value === cell) spotted.value = null
  afterEdit()
}

/** 收回一個指定。指錯了一定要收得回來——對帳會把綁定當事實。 */
function unassign(cell: string) {
  const g = graph.value
  if (!g) return
  bind(g.getDataModel(), cell, null)
  assigned.value = assigned.value.filter((a) => a.cell !== cell)
  afterEdit()
}

/** 標示圖上的某一個形狀。 */
function spot(cell: string | null) {
  spotted.value = cell
  void repaint()
}

// ── 新增形狀 ────────────────────────────────────────────────────

/**
 * ⚠️ **拖出來的形狀不帶 `loomId`**，所以它會自動落在「還沒指定」那份清單裡。
 * 這是「圖上新增一個框」對模型的意思：不是新增一台機器，
 * 是**多了一件還沒交代的事**。
 */
let made = 0
function insert(shape: Palette) {
  const g = graph.value
  if (!g) return
  const box = g.container.getBoundingClientRect()
  const at = g.getPointForEvent({
    clientX: box.left + box.width / 2,
    clientY: box.top + box.height / 2,
  } as MouseEvent)

  g.batchUpdate(() => {
    const cell = g.insertVertex({
      parent: g.getDefaultParent(),
      id: `new-${made += 1}`,
      value: shape.label,
      position: [Math.round(at.x - shape.width / 2), Math.round(at.y - shape.height / 2)],
      size: [shape.width, shape.height],
      style: parseStyle(shape.style),
    })
    g.setSelectionCell(cell)
  })
  afterEdit()
}

// ── 接線 ────────────────────────────────────────────────────────

function afterEdit() {
  unsaved.value = true
  refreshUnbound()
  void refreshTargets()
}

const clip = computed(() => {
  const g = graph.value
  return g ? clipboard(g, afterEdit) : null
})

let detachKeyboard: (() => void) | null = null

/** 畫布備好之後才掛得上右鍵選單與鍵盤——它們都要 graph。 */
watch(graph, (g) => {
  detachKeyboard?.()
  if (!g || !clip.value) return
  const openData = (cell: Cell) => { editing.value = cell }
  contextMenu(g, { clip: clip.value, onEditData: openData, onChange: afterEdit })
  detachKeyboard = keyboard(g, canvas.value!.undo!, {
    clip: clip.value,
    onEditData: openData,
    onChange: afterEdit,
  })
  void reopen()
})

// 換環境等於換一張圖：一張圖只屬於一個環境。
watch(() => environment.value?.id, () => { opened.value = null; void reopen() })

/**
 * 模型變了。
 *
 * **存過的圖不自動重產**——重產會把使用者排好的版面洗掉，而他可能只是在
 * 另一頁改了一個位址。改成把「這張圖是舊的」講出來，要不要重畫由他決定。
 */
watch(() => store.snapshot, async () => {
  await refreshCatalog()
  if (!onDisk.value && isGenerated(showing.value)) await regenerate()
})

// 換篩選只重塗，不重畫——重畫會洗掉手工排好的版面。
watch(() => [picked.value, store.onlyProblems], () => void repaint(), { deep: true })

// 圖一改，可以指給誰就變了（剛剛用掉的那個元素不能再指第二次）。
watch([binderOpen, unbound], () => void refreshTargets())

function toggleFilter() {
  filterOpen.value = !filterOpen.value
  if (filterOpen.value) toggleBinder(false)
}

function toggleBinder(open = !binderOpen.value) {
  binderOpen.value = open
  if (open) { filterOpen.value = false; refreshUnbound() }
  else if (spotted.value) spot(null)
}

function onCanvasChange() {
  unsaved.value = true
  refreshUnbound()
}

function onLaid(ms: number) {
  status.value = `排版 ${Math.round(ms)} ms`
  refreshUnbound()
  // 換了一張圖，上一張的搜尋結果全部失效——那些 cell id 已經不在了。
  refreshLabels()
  if (needle.value) goTo(0)
}
</script>

<template>
  <div class="wrap">
    <p v-if="!environment" class="empty muted">
      還沒有環境。一張部署圖畫的是「某個環境裡東西跑在哪」，所以要先建一個環境。
    </p>

    <template v-else>
      <div class="bar">
        <button title="復原 ⌘Z" @click="canvas?.undo?.undo()">↶</button>
        <button title="重做 ⇧⌘Z" @click="canvas?.undo?.redo()">↷</button>
        <span class="sep" />
        <button title="放大" @click="graph?.zoomIn()">＋</button>
        <button title="縮小" @click="graph?.zoomOut()">－</button>
        <button title="整張圖" @click="canvas?.fit()">⤢</button>
        <span class="sep" />
        <button class="filter" :class="{ on: pickerOpen }" @click="pickerOpen = !pickerOpen">形狀</button>
        <button class="filter" :class="{ on: filtering }" @click="toggleFilter()">
          篩選<span v-if="filtering" class="count">{{ litCount }}/{{ totalShapes }}</span>
        </button>
        <!-- 沒指定的形狀對帳看不見。數字掛在按鈕上，使用者才有理由打開它。 -->
        <button class="filter" :class="{ on: unbound.length > 0 }" @click="toggleBinder()">
          標註<span v-if="unbound.length" class="count">{{ unbound.length }}</span>
        </button>
        <span class="sep" />

        <!--
          找形狀。**它是「帶你過去」，不是篩選**——圖上藏掉幾個框會讓剩下的
          線指向空氣。所以這裡沒有任何東西會消失，只有畫面移過去。

          Enter 下一個、⇧Enter 上一個，跟一般編輯器一樣。⎋ 清掉。
        -->
        <span class="find" :class="{ miss: needle.trim() !== '' && hits.length === 0 }">
          <input
            v-model="needle"
            type="search"
            class="needle"
            placeholder="在圖上找…"
            @keydown.enter.prevent="stepBy($event.shiftKey ? -1 : 1)"
            @keydown.esc.prevent="clearSearch()"
          >
          <!-- 「第幾個／共幾個」一定要寫出來：只把畫面移過去的話，
               使用者不知道還有沒有別的，也不知道自己在第幾個。 -->
          <template v-if="needle.trim()">
            <span v-if="hits.length" class="count">{{ at + 1 }}/{{ hits.length }}</span>
            <span v-else class="count">找不到</span>
            <button v-if="hits.length > 1" title="上一個 ⇧Enter" @click="stepBy(-1)">↑</button>
            <button v-if="hits.length > 1" title="下一個 Enter" @click="stepBy(1)">↓</button>
          </template>
        </span>

        <span class="sep" />

        <select v-if="store.environments.length > 1" :value="environment.id" @change="environmentChosen = ($event.target as HTMLSelectElement).value">
          <option v-for="e in store.environments" :key="e.id" :value="e.id">{{ e.slug }}</option>
        </select>
        <span v-else class="muted">{{ environment.slug }}</span>

        <select :value="showing" @change="openDiagram(($event.target as HTMLSelectElement).value)">
          <option :value="deployment">{{ deployment }}（自動產生）</option>
          <option :value="context">{{ context }}（自動產生 · 簡圖）</option>
          <option v-for="d in diagrams.filter((x) => !x.generated)" :key="d.name" :value="d.name">{{ d.name }}</option>
        </select>
        <template v-if="newName === null">
          <button @click="newName = ''">新增圖</button>
        </template>
        <template v-else>
          <input v-model="newName" class="name" placeholder="新圖的名字" @keydown.enter="createDiagram(newName ?? '')" @keydown.esc="newName = null">
          <button @click="createDiagram(newName ?? '')">建立</button>
          <button @click="newName = null">取消</button>
        </template>

        <!--
          詳圖／簡圖。**一定要說出來。**

          「不對帳」如果只是程式內部安靜地跳過，那就是這個專案最怕的那種
          失敗：有人把簡圖貼進文件，同事看到它從這個工具長出來，理所當然
          以為它被檢查過了。所以它是一顆看得見的按鈕，不是設定裡的一個選項。

          App 產的那張改不了——它是程式照模型畫的，一個形狀就是一個元素，
          那不是偏好。
        -->
        <button
          class="filter kind" :class="{ simple: kind === 'simple' }"
          :disabled="isGenerated(showing)"
          :title="isGenerated(showing)
            ? (showing === deployment
              ? '部署圖永遠是詳圖：一個形狀剛好一個模型元素'
              : 'context 圖永遠是簡圖：兩個系統之間的線是收攏過的，一條線代表一群契約')
            : kind === 'detail'
              ? '詳圖：一個形狀剛好一個模型元素，會完整對帳。點一下改成簡圖'
              : '簡圖：一個形狀可以代表一群，**不對帳**（只檢查引用的東西還在不在）。點一下改成詳圖'"
          @click="setKind(kind === 'detail' ? 'simple' : 'detail')"
        >{{ kind === 'detail' ? '詳圖' : '簡圖 · 不對帳' }}</button>

        <span class="muted small">{{ drawn.length }} 條線</span>
        <!--
          收起來的東西**一定要說出來**。這個工具存在的理由就是怕漏，
          而折疊是主動把東西藏起來——有人把一張折疊過的圖貼進文件，
          讀的人要看得出來這不是全部。
        -->
        <button
          v-if="hiddenCount > 0"
          class="filter simple"
          title="這張圖收起了一些框，底下的東西沒有畫出來。點一下全部展開"
          @click="collapsed = new Set()"
        >收起 {{ hiddenCount }} 個 · 全部展開</button>
        <span v-if="tooMany" class="warn small">太多了，這張圖已經讀不動——用篩選</span>

        <span class="grow" />
        <span v-if="unsaved" class="warn small" title="這張圖改過還沒存">● 未存檔</span>
        <span class="muted small">{{ status }}</span>
        <button :disabled="!graph" @click="saveDiagram()">存檔</button>
        <button :disabled="redrawing" @click="regenerate()">重畫</button>
      </div>

      <div class="body">
        <ShapePicker v-if="pickerOpen" @pick="insert" @close="pickerOpen = false" />

        <div class="canvas">
          <!-- 模型變了，這張圖畫的是舊的。存檔會被擋下來，所以先講。 -->
          <p v-if="stale" class="hint">
            <strong>模型已經變了，這張圖畫的是舊的模型。</strong>
            所以現在存檔會被擋下來。按<strong>重畫</strong>會用現在的模型重產一張——
            代價是這張圖上手工排的版面與標註會不見。
          </p>
          <p v-if="empty" class="hint">
            <strong>{{ environment.slug }} 還沒有任何機器。</strong>
            <template v-if="showing === context">
              context 圖畫的是「有哪些系統」，而一個系統要在這個環境有實體才算數。
            </template>
            <template v-else>
              部署圖畫的是「東西跑在哪裡」。
            </template>
            所以要先到<strong>資源</strong>那一頁建機器與服務實體，這裡才畫得出東西。
          </p>

          <DiagramCanvas
            ref="canvas"
            :shapes="shapes"
            :links="drawn"
            :xml="xml"
            @fold="onFold"
            @change="onCanvasChange"
            @select="selection = $event"
            @laid="onLaid"
          />

          <DiagramFilter
            v-if="filterOpen"
            :contracts="contracts" :picked="picked" :problems="store.onlyProblems"
            :lit="litCount" :total="totalShapes"
            @update:picked="picked = $event"
            @update:problems="store.onlyProblems = $event"
            @close="filterOpen = false"
          />
          <DiagramBinder
            v-if="binderOpen"
            :shapes="unbound" :annotation="annotation" :assigned="assigned" :spot="spotted"
            @assign="assign" @unassign="unassign" @spot="spot" @close="toggleBinder()"
          />
        </div>

        <FormatPanel :graph="graph" :selection="selection" @change="afterEdit" />
      </div>

      <MetadataDialog
        v-if="editing && graph"
        :graph="graph" :cell="editing"
        @change="afterEdit" @close="editing = null"
      />
    </template>
  </div>
</template>

<style scoped>
.wrap { flex: 1; min-height: 0; display: flex; flex-direction: column; background: var(--surface); }

.bar {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 12px;
  border-bottom: 1px solid var(--rule);
  background: var(--surface-2);
  font-size: 12.5px;
}
.bar button { height: 26px; min-width: 28px; padding: 0 8px; cursor: pointer; }
.grow { flex: 1; }
.small { font-size: 11.5px; }
.sep { width: 1px; height: 18px; margin: 0 4px; background: var(--rule); }
.name { width: 140px; }

.body { flex: 1; min-height: 0; display: flex; }
/* 篩選與標註是浮在畫布上的，所以這裡要當定位的基準。 */
.canvas { flex: 1; min-width: 0; display: flex; flex-direction: column; position: relative; }

.filter.on { border-color: var(--warp); color: var(--warp); }
/* 簡圖要一眼看得出來——它不對帳，而「沒說 = 沒問題」的誤會太容易發生。
   用警告色而不是強調色：這不是一個開著的功能，是一個要知道的限制。 */
.kind.simple { border-color: var(--warn); color: var(--warn); }
.kind:disabled { opacity: .55; }
.filter .count { margin-left: 6px; font-size: 11px; font-variant-numeric: tabular-nums; opacity: 0.8; }

/* 找形狀。「第幾個／共幾個」用等寬數字，否則走到 10 的時候整排會抖一下。 */
.find { display: inline-flex; align-items: center; gap: 4px; }
.needle { width: 150px; }
.find .count {
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  color: var(--ink-3);
  min-width: 3.5ch;
}
/* 找不到要看得出來。只讓數字說「找不到」的話，眼睛還在圖上的人不會注意到。 */
.find.miss .needle { border-color: var(--warn); }
.find.miss .count { color: var(--warn); }
.find button { padding: 1px 6px; font-size: 11px; }

.hint {
  margin: 0;
  padding: 10px 14px;
  border-bottom: 1px solid var(--rule);
  background: color-mix(in srgb, var(--warn) 10%, transparent);
  font-size: 12.5px;
  line-height: 1.7;
}
.warn { color: var(--broken); }

.empty { padding: 40px 24px; text-align: center; max-width: 46ch; margin: 0 auto; line-height: 1.7; }
</style>
