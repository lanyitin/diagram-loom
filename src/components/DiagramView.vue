<script setup lang="ts">
/**
 * 內嵌的 draw.io 編輯器。
 *
 * # 協定的三個坑（階段 0 實測，見 `docs/stage0-findings.md`）
 *
 * 1. **v31 已經沒有 `save` 事件了。** 網路上找得到的文件都說有，
 *    但原始碼裡搜不到那個字串。要知道使用者改了什麼，得用
 *    `{action:'load', autosave:1}` 之後收 `autosave` 事件。
 * 2. **取模型一定要用 `format:'xml'`。** `xmlsvg` 也附一個 `xml` 欄位，
 *    但那個永遠是壓縮的，`loomId` 整個看不見。第一輪就是被這個騙到。
 * 3. **不要傳 `offline=1`。** 傳了會把 service worker 招來。
 *
 * # 為什麼 iframe 在別的 origin 是好事
 *
 * draw.io 那包 152 MB，走 `drawio://` 自訂協定從磁碟供應（放進
 * `frontendDist` 會嵌進執行檔）。副作用是它落在自己的 origin 上——
 * 而嵌入協定本來就是為跨來源設計的，主視窗的 CSP 也因此不會意外綁住它。
 */
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { commands } from '../lib/bindings'
import { useProject } from '../lib/store'
import {
  bind,
  blankXml,
  boundShapes,
  dim,
  shapesOf,
  spotlight,
  toXml,
  unbind,
  unboundShapes,
  undim,
} from '../lib/diagram'
import DiagramBinder from './DiagramBinder.vue'
import DiagramFilter from './DiagramFilter.vue'
import type { Annotation, Contract, DiagramInfo, Highlight, Link } from '../lib/model'

const store = useProject()

/**
 * 實測過可用的參數（階段 0）。
 *
 * `proto=json` 是整個嵌入協定的開關；`configure=1` 讓我們在 `configure`
 * 事件裡回一份設定；`stealth=1` 擋掉所有對外連線。
 */
const PARAMS =
  'embed=1&proto=json&configure=1&stealth=1&spin=1&noSaveBtn=1&noExitBtn=1&libraries=1'

const frame = ref<HTMLIFrameElement | null>(null)
const ready = ref(false)
const status = ref('')
const links = ref<Link[]>([])
const drawing = ref(false)

/** 篩選：勾了哪些契約、要不要只看有問題的。 */
const picked = ref<string[]>([])
const contracts = ref<Contract[]>([])
const highlight = ref<Highlight | null>(null)
const totalShapes = ref(0)

/** 最後一次交給編輯器的 XML。調暗改的是它，不是重新產一張。 */
const current = ref('')

/**
 * 存檔。
 *
 * # 一個環境一張自動產生的詳圖，其他隨使用者建
 *
 * App 產的那張叫什麼由 Rust 給（`details`），前端不自己抄一份名字——
 * 抄了就會有兩個「保留字」，而改名的那天只會有一邊跟著改。
 *
 * # 存檔時模型變了就不存
 *
 * 圖畫的是「當時的模型」。模型後來改了，這張圖上的名字、位址、有哪些框
 * 就都是舊的。判斷在 Rust（`diagrams.rs`），這裡只負責把「畫這張圖時的
 * 模型指紋」原樣送回去，以及**把它拒絕的理由講給使用者聽**。
 */
const diagrams = ref<DiagramInfo[]>([])
const details = ref('diagram-loom-details')
/** 現在開著哪一張。`null` 表示還沒決定，開專案時會落在詳圖上。 */
const opened = ref<string | null>(null)
/** 這張圖畫的是哪一版模型。存檔時要拿它跟現在的比。 */
const drawnFrom = ref('')
/** 模型現在的指紋。跟上面那個對不上，就代表這張圖是舊的。 */
const modelNow = ref('')
/** 改過但還沒存。只是提醒，不擋任何操作。 */
const unsaved = ref(false)
/** 正在打字的新圖名字。`null` 表示沒有在新增。 */
const newName = ref<string | null>(null)

const showing = computed(() => opened.value ?? details.value)
/** 這張圖存過了沒。沒存過的那張詳圖才可以在模型變動時自動重產。 */
const onDisk = computed(() => diagrams.value.some((d) => d.name === showing.value))
/** 這張圖畫的是舊的模型。 */
const stale = computed(() => Boolean(drawnFrom.value) && drawnFrom.value !== modelNow.value)

const filtering = computed(() => picked.value.length > 0 || store.onlyProblems)
/** 篩選面板打開了沒。關起來的時候一個像素都不佔——不然會擠到 draw.io 自己的面板。 */
const filterOpen = ref(false)
const litCount = computed(() => highlight.value?.shapes.length ?? 0)

/**
 * 標註：圖上還沒指定代表誰的形狀。
 *
 * 使用者自己畫的框對帳看不見，所以「圖上有畫」不等於「這件事管到了」。
 * 這份清單就是把那些漏網之魚列出來，一個一個指給模型元素。
 */
const binderOpen = ref(false)
/** 現在圖上標示著哪一個形狀。同時只標一個——標一堆等於沒標。 */
const spotted = ref<string | null>(null)
/** 這一輪指定了哪些，留著讓人收得回來。 */
const assigned = ref<{ cell: string; label: string; target: string }[]>([])
const annotation = ref<Annotation>({ shapes: [], connections: [], guesses: [] })

/** 圖上還沒指定的形狀。由編輯器最後交回來的那份 XML 算出來。 */
const unbound = computed(() => (current.value ? unboundShapes(current.value) : []))

/**
 * 我們在圖上塗過螢光筆沒有。
 *
 * 塗過才需要擦——`opacity` 已經寫進編輯器交回來的 XML 了，不擦的話關掉篩選
 * 圖還是暗的。但沒塗過就不能亂擦：那會把**使用者自己調的半透明**洗掉。
 */
const smeared = ref(false)

/**
 * 這個環境一台機器都沒有。
 *
 * 空的部署圖跟壞掉的編輯器長得一模一樣——都是一片空白格線。
 * 使用者第一個念頭會是「工具壞了」，而不是「這裡還沒填東西」。
 */
const empty = computed(() => {
  const env = environment.value
  if (!env || !store.snapshot) return false
  return shapesOf(store.snapshot.project, env).length === 0
})

/**
 * 一條萬用字元連線是 N×M 條線——12 台連 12 台就是 144 條。
 * 超過這個數字，圖已經沒有人讀得動了，該用篩選（尚未實作）而不是硬畫。
 */
const TOO_MANY = 600
const tooMany = computed(() => links.value.length > TOO_MANY)

/** 排版跑多久算是沒回應。實測 2480 個節點是 6 秒（`docs/scale-limits.md`）。 */
const LAYOUT_TIMEOUT = 15000
/** 一連串改動之間隔多久算是停了。 */
const SETTLE = 400
/** 正在等排版跑完。等到了才縮放。 */
const waiting = ref(false)
let deadline = 0
let settle = 0

/**
 * 現在畫的是哪個環境。
 *
 * **一張圖只屬於一個環境**，所以這裡要自己選一個，不能跟著上面那排
 * 勾選走——兩個都勾的時候，跟著走就永遠只看得到第一個，另一個
 * 沒有任何辦法可以打開。
 */
const chosen = ref<string | null>(null)
const environment = computed(
  () =>
    store.environments.find((e) => e.id === chosen.value)
    ?? store.visibleEnvironments[0]
    ?? store.environments[0]
    ?? null,
)

function send(message: unknown) {
  frame.value?.contentWindow?.postMessage(JSON.stringify(message), '*')
}

/**
 * 我們自己送進去的 `load` 會引來一則 `autosave`（排版跑完還會再一則）。
 * 那些不是使用者的編輯，所以先記著要吃掉幾則——不然一打開圖就標成「未存檔」。
 */
let expecting = 0

function fill(xml: string, autosave = 1) {
  expecting += 1
  send({ action: 'load', xml, autosave })
}

/** 換一張圖給編輯器。上一張留下的標註、螢光筆、燈全部歸零。 */
async function show(xml: string, model: string, layout: boolean) {
  current.value = xml
  drawnFrom.value = model
  // 換圖等於換一張畫布，上一張的東西全部不在了：指定過的形狀、標示的燈、
  // 塗上去的螢光筆。留著只會指到空氣。
  assigned.value = []
  spotted.value = null
  smeared.value = false
  unsaved.value = false

  // 沒有 save 事件，所以靠 autosave 才知道使用者動了什麼。
  fill(await painted())
  if (layout) {
    expecting += 1
    relayout()
  }
}

/** 這個環境有哪些圖、模型現在的指紋。 */
async function refreshCatalog() {
  const env = environment.value
  if (!env) return

  const res = await commands.diagramCatalog(env.id)
  if (res.status !== 'ok') {
    store.error = (res.error as { message?: string })?.message ?? String(res.error)
    return
  }
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
  else if (name === details.value) await loadModel()
  else store.error = `找不到圖 ${name}`
}

async function readDiagram(name: string) {
  const env = environment.value
  if (!ready.value || !env) return

  const res = await commands.diagramRead(env.id, name)
  if (res.status !== 'ok') {
    store.error = (res.error as { message?: string })?.message ?? String(res.error)
    return
  }
  opened.value = name
  // 存過的圖帶著使用者排好的座標，所以**不重新排版**——排下去等於把他的版面洗掉。
  await show(res.data.xml, res.data.model, false)
}

/**
 * 從模型重產那張詳圖。
 *
 * ⚠️ 這會蓋掉使用者在這張圖上排的版面。所以只有兩種情況會走到這裡：
 * 使用者自己按「重畫」，或者這張詳圖還沒存過（沒有版面可以蓋掉）。
 */
async function loadModel() {
  const env = environment.value
  if (!ready.value || !env || !store.snapshot || drawing.value) return

  // 先把指紋更新到最新的。拿舊的當「這張圖畫的是哪一版」，使用者會卡在
  // 「重畫 → 存檔被擋 → 再重畫」的迴圈裡，而且畫面上完全看不出為什麼。
  await refreshCatalog()

  drawing.value = true
  try {
    // 萬用字元展開成哪幾台是模型知識，所以由 Rust 算（`wiring.rs`）。
    // 在這裡自己比對一次 slug，就會養出第二套「什麼叫比對得上」。
    const res = await commands.diagramLinks(env.id)
    if (res.status !== 'ok') {
      store.error = (res.error as { message?: string })?.message ?? String(res.error)
      return
    }
    links.value = res.data
  } finally {
    drawing.value = false
  }

  opened.value = details.value
  await show(toXml(store.snapshot.project, env, links.value), modelNow.value, true)
}

/** 存檔用的那一份：把螢光筆擦掉。塗過的 `opacity` 存下去會變成永久的。 */
function clean(): string {
  return smeared.value ? undim(current.value) : current.value
}

/**
 * 存檔。**模型變了 Rust 會擋下來**，這裡負責把理由講給使用者聽。
 */
async function saveDiagram() {
  const env = environment.value
  if (!env || !current.value) return

  const res = await commands.diagramSave(env.id, showing.value, clean(), drawnFrom.value)
  if (res.status !== 'ok') {
    store.error = (res.error as { message?: string })?.message ?? String(res.error)
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
  if (res.status !== 'ok') {
    store.error = (res.error as { message?: string })?.message ?? String(res.error)
    return
  }
  newName.value = null
  await refreshCatalog()
  await readDiagram(res.data.name)
}

/**
 * 排版，然後等它真的跑完才縮放。
 *
 * ⚠️ **`fit` 不可以緊接著 `layout` 送。** ELK 是非同步的（`executeLayoutSpec`
 * 還會先等 ELK 那包載完），所以緊接著縮放的是**排版前**的畫面。
 *
 * ⚠️ 而且**排版失敗時沒有任何錯誤事件**（見 `docs/nested-layout.md`），
 * 所以一定要有逾時，否則就是無聲地什麼都沒發生。
 */
function relayout() {
  // 只有 layered 處理得了巢狀節點，而我們的圖天生巢狀（站點 → 機器 → 服務實體）。
  send({ action: 'layout', layouts: 'verticalFlow' })

  waiting.value = true
  clearTimeout(deadline)
  deadline = window.setTimeout(() => {
    if (!waiting.value) return
    stopWaiting()
    // 沒有錯誤事件，所以這句話是使用者唯一的線索。
    status.value = '排版沒有回應，先照原樣顯示'
    send({ action: 'fit', maxScale: 1.2 })
  }, LAYOUT_TIMEOUT)
}

function stopWaiting() {
  waiting.value = false
  clearTimeout(deadline)
  clearTimeout(settle)
}

/**
 * 縮放到**這一連串改動的最後一筆**。
 *
 * `load` 自己也會送一次 `autosave`，而它比排版早到。認第一筆的話，
 * 縮放的還是排版前的畫面——跟原本那個 bug 一模一樣，只是換個地方犯。
 * 所以每來一筆就把計時器往後推，等真的安靜下來才縮放。
 */
function fitWhenSettled() {
  clearTimeout(settle)
  settle = window.setTimeout(() => {
    stopWaiting()
    send({ action: 'fit', maxScale: 1.2 })
  }, SETTLE)
}

/**
 * 去問 Rust「誰該亮」，然後把螢光筆塗上去。
 *
 * 「勾了這條契約，哪些東西算相關」跟 lint 是同一類判斷，所以在 Rust
 * （`highlight.rs`）。在這裡自己算會養出第二套「什麼叫相關」。
 */
async function painted(): Promise<string> {
  const env = environment.value
  if (!env) return current.value

  // 每次都從**擦乾淨的**那份開始塗。不然上一次塗的 opacity 已經寫進 XML 了，
  // 這次只會疊在上面——關掉篩選之後圖還是暗的，而症狀看起來像是「按了沒反應」。
  const base = smeared.value ? undim(current.value) : current.value

  // 標示某一個形狀時，螢光筆歸標註用。兩支筆同時塗，出來的顏色沒有人讀得懂。
  if (spotted.value) {
    smeared.value = true
    return spotlight(base, spotted.value)
  }

  const res = await commands.diagramFocus(env.id, {
    relationships: picked.value,
    problems: store.onlyProblems,
  })
  if (res.status !== 'ok') {
    store.error = (res.error as { message?: string })?.message ?? String(res.error)
    return base
  }

  contracts.value = res.data.contracts
  highlight.value = res.data.highlight
  totalShapes.value = res.data.shapes

  // 什麼都沒勾就不塗——全部塗成「亮」等於把使用者自己調的半透明洗掉。
  if (!filtering.value) return base
  smeared.value = true
  const lit = new Set([...res.data.highlight.shapes, ...res.data.highlight.connections])
  return dim(base, lit)
}

/**
 * 去問 Rust「這些沒指定的形狀可以指給誰」。
 *
 * 「哪些元素還沒被用掉」跟對帳是同一類判斷，所以在 Rust（`annotate.rs`）。
 * 這裡只負責說「圖上有這些形狀」——跟對帳同一條分界線。
 */
async function refreshTargets() {
  const env = environment.value
  if (!binderOpen.value || !env || !current.value) return

  const res = await commands.diagramTargets(
    env.id,
    boundShapes(current.value).map((s) => s.id),
    unbound.value,
  )
  if (res.status !== 'ok') {
    store.error = (res.error as { message?: string })?.message ?? String(res.error)
    return
  }
  annotation.value = res.data
}

/**
 * 兩片面板互斥：都浮在畫布左上角，同時開會疊在一起。
 *
 * 關掉標註時要順手把燈熄掉——面板不見了、圖上還暗著一半，
 * 使用者會以為圖壞了，而且找不到是哪裡把它弄暗的。
 */
function toggleFilter() {
  filterOpen.value = !filterOpen.value
  if (filterOpen.value) toggleBinder(false)
}

function toggleBinder(open = !binderOpen.value) {
  binderOpen.value = open
  if (open) filterOpen.value = false
  else if (spotted.value) spot(null)
}

/** 標示圖上的某一個形狀。重塗一次換一次標示——所以是按的，不是滑過去的。 */
async function spot(cell: string | null) {
  spotted.value = cell
  await repaint()
}

/** 指定一個形狀代表誰。改的是圖，不是模型——模型一個字都沒動。 */
async function assign(cell: string, target: string) {
  const shape = unbound.value.find((s) => s.cell === cell)
  current.value = bind(current.value, cell, target)
  assigned.value = [...assigned.value, { cell, label: shape?.label ?? '', target }]
  // 指定完就把標示收掉：那個形狀已經從清單上消失了，燈還亮著只會讓人找不到自己在哪。
  if (spotted.value === cell) spotted.value = null
  await repaint()
}

/** 收回一個指定。指錯了一定要收得回來——對帳會把綁定當事實。 */
async function unassign(cell: string) {
  current.value = unbind(current.value, cell)
  assigned.value = assigned.value.filter((a) => a.cell !== cell)
  await repaint()
}

/**
 * 只重塗，不重畫。
 *
 * 從模型重產會洗掉使用者手工排好的版面——按一下篩選，半小時的拖拉就沒了。
 * 所以改的是編輯器最後給我們的那份 XML（座標都在裡面），只動 `opacity`。
 */
async function repaint() {
  if (!ready.value || !current.value) return
  fill(await painted())
}

function onMessage(event: MessageEvent) {
  // iframe 在別的 origin，所以來源比對不了。改用「訊息長得對不對」來擋——
  // 這個視窗裡本來就只有 draw.io 會送 JSON 字串進來。
  if (event.source !== frame.value?.contentWindow) return
  if (typeof event.data !== 'string' || !event.data.startsWith('{')) return

  let message: { event?: string; xml?: string }
  try {
    message = JSON.parse(event.data)
  } catch {
    return
  }

  switch (message.event) {
    case 'configure':
      send({ action: 'configure', config: { defaultFonts: ['Helvetica'] } })
      break
    case 'init':
      ready.value = true
      reopen()
      break
    case 'autosave':
      // 使用者動了圖。**這裡不自動寫回模型**——圖與模型誰都不是老大，
      // 要由人裁決（階段 7 的對帳面板）。這裡只記下座標，以及「還沒存檔」。
      if (message.xml) {
        // 記住座標。下次套篩選要改的是這一份，不是從模型重產。
        current.value = message.xml
        // 我們自己送的 `load` 也會引來一則，那些不算使用者的編輯。
        if (expecting > 0) expecting -= 1
        else unsaved.value = true
        const left = unbound.value.length
        // 「還有幾個沒指定」要一直看得到，不能只在標註面板裡講：
        // 使用者不會去打開一個他不知道自己需要的面板。
        status.value = `圖上有 ${boundShapes(message.xml).length} 個綁定的形狀`
          + (left ? `，${left} 個沒指定` : '')
      }
      // 排版跑完會改動內容，於是送這個事件過來——但 `load` 自己也會送一次，
      // 而且比排版早到。所以等安靜下來才縮放。
      if (waiting.value) fitWhenSettled()
      break
  }
}

// 換環境等於換一張圖：一張圖只屬於一個環境。
watch(() => environment.value?.id, () => { opened.value = null; reopen() })

/**
 * 模型變了。
 *
 * **存過的圖不自動重產**——重產會把使用者排好的版面洗掉，而他可能只是在
 * 另一頁改了一個位址。改成把「這張圖是舊的」講出來，要不要重畫由他決定。
 *
 * 還沒存過的那張詳圖沒有版面可以損失，所以照舊自動跟上。
 */
watch(() => store.snapshot, async () => {
  await refreshCatalog()
  if (!onDisk.value && showing.value === details.value) await loadModel()
})

// 換篩選只重塗，不重畫——重畫會洗掉手工排好的版面。
watch(() => [picked.value, store.onlyProblems], () => repaint(), { deep: true })

// 圖一改，可以指給誰就變了（剛剛用掉的那個元素不能再指第二次）。
watch([binderOpen, unbound], () => refreshTargets(), { immediate: true })

// 掛在 mounted 而不是 iframe 的 load：load 每重載一次就多掛一個，
// 而多掛的那些不會壞掉，只會讓每則訊息被處理很多次——很難查。
onMounted(() => window.addEventListener('message', onMessage))
onUnmounted(() => {
  window.removeEventListener('message', onMessage)
  stopWaiting()
})
</script>

<template>
  <div class="wrap">
    <p v-if="!environment" class="empty muted">
      還沒有環境。一張部署圖畫的是「某個環境裡東西跑在哪」，所以要先建一個環境。
    </p>

    <template v-else>
      <div class="bar">
        <!-- 兩片面板都浮在畫布左上角，同時開會疊在一起，所以互斥。 -->
        <button class="filter" :class="{ on: filtering }" @click="toggleFilter()">
          篩選<span v-if="filtering" class="count">{{ litCount }}/{{ totalShapes }}</span>
        </button>
        <!-- 沒指定的形狀對帳看不見。數字掛在按鈕上，使用者才有理由打開它。 -->
        <button class="filter" :class="{ on: unbound.length > 0 }" @click="toggleBinder()">
          標註<span v-if="unbound.length" class="count">{{ unbound.length }}</span>
        </button>
        <select v-if="store.environments.length > 1" :value="environment.id" @change="chosen = ($event.target as HTMLSelectElement).value">
          <option v-for="e in store.environments" :key="e.id" :value="e.id">{{ e.slug }}</option>
        </select>
        <span v-else class="muted">{{ environment.slug }}</span>

        <!-- 哪一張圖。App 產的那張永遠在，即使還沒存過。 -->
        <select :value="showing" @change="openDiagram(($event.target as HTMLSelectElement).value)">
          <option :value="details">{{ details }}（自動產生）</option>
          <option v-for="d in diagrams.filter((x) => !x.generated)" :key="d.name" :value="d.name">
            {{ d.name }}
          </option>
        </select>
        <!-- 新增只問名字。空白畫布由使用者自己畫，或把畫好的貼進來。 -->
        <template v-if="newName === null">
          <button @click="newName = ''">新增圖</button>
        </template>
        <template v-else>
          <input
            v-model="newName" class="name" placeholder="新圖的名字" autofocus
            @keydown.enter="createDiagram(newName ?? '')"
            @keydown.esc="newName = null"
          >
          <button @click="createDiagram(newName ?? '')">建立</button>
          <button @click="newName = null">取消</button>
        </template>

        <span class="muted small">{{ links.length }} 條線</span>
        <!-- 萬用字元是 N×M，所以「幾條連線」跟「圖上幾條線」差很多。
             不講的話使用者只會覺得圖突然變成一團毛線，不知道為什麼。 -->
        <span v-if="tooMany" class="warn small">
          太多了，這張圖已經讀不動——篩選功能還沒做
        </span>
        <span class="grow" />
        <span v-if="unsaved" class="warn small" title="這張圖改過還沒存">● 未存檔</span>
        <span class="muted small">{{ status }}</span>
        <button :disabled="!ready || !current" @click="saveDiagram()">存檔</button>
        <button :disabled="!ready || drawing" @click="loadModel()">重畫</button>
      </div>
      <div class="body">
        <div class="canvas">
          <DiagramFilter
            v-if="filterOpen"
            :contracts="contracts"
            :picked="picked"
            :problems="store.onlyProblems"
            :lit="litCount"
            :total="totalShapes"
            @update:picked="picked = $event"
            @update:problems="store.onlyProblems = $event"
            @close="filterOpen = false"
          />
          <DiagramBinder
            v-if="binderOpen"
            :shapes="unbound"
            :annotation="annotation"
            :assigned="assigned"
            :spot="spotted"
            @assign="assign"
            @unassign="unassign"
            @spot="spot"
            @close="toggleBinder()"
          />
          <!-- 模型變了，這張圖畫的是舊的。存檔會被擋下來，所以先講。
               不講的話，使用者會按了存檔才發現，而且不知道是哪裡變了。 -->
          <p v-if="stale" class="hint">
            <strong>模型已經變了，這張圖畫的是舊的模型。</strong>
            所以現在存檔會被擋下來。按<strong>重畫</strong>會用現在的模型重產一張——
            代價是這張圖上手工排的版面與標註會不見。
          </p>
          <!-- 空的部署圖跟壞掉的編輯器長得一樣，都是一片空白格線。
               不講的話使用者會以為是工具壞了。 -->
          <p v-if="empty" class="hint">
            <strong>{{ environment.slug }} 還沒有任何機器。</strong>
            部署圖畫的是「東西跑在哪裡」，所以要先到<strong>資源</strong>那一頁
            建機器與服務實體，這裡才畫得出東西。
          </p>
          <iframe
            ref="frame"
            class="editor"
            :src="`drawio://localhost/index.html?${PARAMS}`"
          />
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.wrap { flex: 1; min-height: 0; display: flex; flex-direction: column; background: var(--surface); }

.bar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 12px;
  border-bottom: 1px solid var(--rule);
  background: var(--surface-2);
  font-size: 12.5px;
}
.grow { flex: 1; }
.small { font-size: 11.5px; }

.body { flex: 1; min-height: 0; display: flex; }
/* 篩選是浮在這上面的，所以要當定位的基準。 */
.canvas { flex: 1; min-width: 0; display: flex; flex-direction: column; position: relative; }

.filter.on { border-color: var(--warp); color: var(--warp); }
.name { width: 140px; }
.filter .count {
  margin-left: 6px;
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  opacity: 0.8;
}

.editor {
  flex: 1;
  min-height: 0;
  border: 0;
  width: 100%;
  /*
   * 把介面縮放抵銷掉。
   *
   * `#app` 上的 `zoom` 會一路蓋到 iframe。draw.io 的版面是 CSS grid，
   * 右邊那欄寫死 240px——它**沒有被擠扁，是被裁掉了**：iframe 內部的版面
   * 比外面的框寬 15%，多出來的部分落在框外被 `overflow: hidden` 切掉，
   * 所以只看得到格式面板最左邊那一條。
   *
   * draw.io 有自己的縮放（右下角、⌘＋），不需要我們這一份。
   *
   * ⚠️ 倒數在 `ui.ts` 算好，**不要寫成 `calc(1 / var(--zoom))`**：`zoom` 是個
   * 老屬性，吃不吃 `calc()` 各家不一致，而解析失敗是靜悄悄的——整條被丟掉，
   * 畫面照舊壞，看不出是這裡的問題。
   */
  zoom: var(--unzoom);
}

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
