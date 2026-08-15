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
import { blankXml, shapesOf } from '../lib/diagram'
import { bind, boundShapes, unboundShapes } from '../lib/graph/binding'
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
import type { Annotation, Contract, DiagramInfo, Highlight, Link } from '../lib/model'

const store = useProject()

const canvas = ref<InstanceType<typeof DiagramCanvas> | null>(null)
const graph = computed(() => canvas.value?.graph ?? null)
const selection = ref<Cell[]>([])
const status = ref('')
const links = ref<Link[]>([])
const drawing = ref(false)

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

/** 這個環境該畫哪些形狀。跟以前同一支——`shapesOf` 已經有測試守著。 */
const shapes = computed(() => {
  const env = environment.value
  return env && store.snapshot ? shapesOf(store.snapshot.project, env) : []
})

/**
 * 這個環境一台機器都沒有。
 *
 * 空的部署圖跟壞掉的編輯器長得一模一樣——都是一片空白格線。
 * 使用者第一個念頭會是「工具壞了」，而不是「這裡還沒填東西」。
 */
const empty = computed(() => shapes.value.length === 0 && !xml.value)

/** 一條萬用字元連線是 N×M 條線——12 台連 12 台就是 144 條。 */
const TOO_MANY = 600
const tooMany = computed(() => links.value.length > TOO_MANY)

// ── 存檔 ────────────────────────────────────────────────────────

const diagrams = ref<DiagramInfo[]>([])
const details = ref('diagram-loom-details')
const opened = ref<string | null>(null)
const drawnFrom = ref('')
const modelNow = ref('')
const unsaved = ref(false)
const newName = ref<string | null>(null)

const showing = computed(() => opened.value ?? details.value)
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
  details.value = res.data.details
}

/** 打開這個環境該顯示的那張圖。 */
async function reopen() {
  await refreshCatalog()
  await openDiagram(showing.value)
}

/**
 * 打開某一張圖：存過的就讀回來，還沒存過的那張詳圖就從模型產。
 *
 * 詳圖在清單裡永遠看得到，即使一次都還沒存過——它是這個環境的主圖，
 * 從清單消失只會讓人以為它不見了。
 */
async function openDiagram(name: string) {
  if (diagrams.value.some((d) => d.name === name)) await readDiagram(name)
  else if (name === details.value) await regenerate()
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
 * 從模型重產那張詳圖。
 *
 * ⚠️ 這會蓋掉使用者在這張圖上排的版面。所以只有兩種情況會走到這裡：
 * 使用者自己按「重畫」，或者這張詳圖還沒存過（沒有版面可以蓋掉）。
 */
async function regenerate() {
  const env = environment.value
  if (!env || !store.snapshot || drawing.value) return

  // 先把指紋更新到最新的。拿舊的當「這張圖畫的是哪一版」，使用者會卡在
  // 「重畫 → 存檔被擋 → 再重畫」的迴圈裡，而且畫面上完全看不出為什麼。
  await refreshCatalog()

  drawing.value = true
  try {
    // 萬用字元展開成哪幾台是模型知識，所以由 Rust 算（`wiring.rs`）。
    // 在這裡自己比對一次 slug，就會養出第二套「什麼叫比對得上」。
    const res = await commands.diagramLinks(env.id)
    if (res.status !== 'ok') return failed(res.error)
    links.value = res.data
  } finally {
    drawing.value = false
  }

  opened.value = details.value
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
  if (!onDisk.value && showing.value === details.value) await regenerate()
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

        <select v-if="store.environments.length > 1" :value="environment.id" @change="environmentChosen = ($event.target as HTMLSelectElement).value">
          <option v-for="e in store.environments" :key="e.id" :value="e.id">{{ e.slug }}</option>
        </select>
        <span v-else class="muted">{{ environment.slug }}</span>

        <select :value="showing" @change="openDiagram(($event.target as HTMLSelectElement).value)">
          <option :value="details">{{ details }}（自動產生）</option>
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

        <span class="muted small">{{ links.length }} 條線</span>
        <span v-if="tooMany" class="warn small">太多了，這張圖已經讀不動——用篩選</span>

        <span class="grow" />
        <span v-if="unsaved" class="warn small" title="這張圖改過還沒存">● 未存檔</span>
        <span class="muted small">{{ status }}</span>
        <button :disabled="!graph" @click="saveDiagram()">存檔</button>
        <button :disabled="drawing" @click="regenerate()">重畫</button>
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
            部署圖畫的是「東西跑在哪裡」，所以要先到<strong>資源</strong>那一頁
            建機器與服務實體，這裡才畫得出東西。
          </p>

          <DiagramCanvas
            ref="canvas"
            :shapes="shapes"
            :links="links"
            :xml="xml"
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
.filter .count { margin-left: 6px; font-size: 11px; font-variant-numeric: tabular-nums; opacity: 0.8; }

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
