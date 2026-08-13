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
import { boundShapes, dim, shapesOf, toXml } from '../lib/diagram'
import DiagramFilter from './DiagramFilter.vue'
import type { Contract, Highlight, Link } from '../lib/model'

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

const filtering = computed(() => picked.value.length > 0 || store.onlyProblems)
/** 篩選面板打開了沒。關起來的時候一個像素都不佔——不然會擠到 draw.io 自己的面板。 */
const filterOpen = ref(false)
const litCount = computed(() => highlight.value?.shapes.length ?? 0)

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

async function loadModel() {
  const env = environment.value
  if (!ready.value || !env || !store.snapshot || drawing.value) return

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

  current.value = toXml(store.snapshot.project, env, links.value)
  // 沒有 save 事件，所以靠 autosave 才知道使用者動了什麼。
  send({ action: 'load', xml: await painted(), autosave: 1 })
  relayout()
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
  // 只有 layered 處理得了巢狀節點，而我們的圖天生巢狀（站點 → 機器 → 落地）。
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

  const res = await commands.diagramFocus(env.id, {
    relationships: picked.value,
    problems: store.onlyProblems,
  })
  if (res.status !== 'ok') {
    store.error = (res.error as { message?: string })?.message ?? String(res.error)
    return current.value
  }

  contracts.value = res.data.contracts
  highlight.value = res.data.highlight
  totalShapes.value = res.data.shapes

  // 什麼都沒勾就不塗——全部塗成「亮」等於把使用者自己調的半透明洗掉。
  if (!filtering.value) return current.value
  const lit = new Set([...res.data.highlight.shapes, ...res.data.highlight.connections])
  return dim(current.value, lit)
}

/**
 * 只重塗，不重畫。
 *
 * 從模型重產會洗掉使用者手工排好的版面——按一下篩選，半小時的拖拉就沒了。
 * 所以改的是編輯器最後給我們的那份 XML（座標都在裡面），只動 `opacity`。
 */
async function repaint() {
  if (!ready.value || !current.value) return
  send({ action: 'load', xml: await painted(), autosave: 1 })
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
      loadModel()
      break
    case 'autosave':
      // 使用者動了圖。**這裡不自動寫回模型**——圖與模型誰都不是老大，
      // 要由人裁決（階段 7 的對帳面板）。現在只記下「有幾個形狀綁著」。
      if (message.xml) {
        // 記住座標。下次套篩選要改的是這一份，不是從模型重產。
        current.value = message.xml
        status.value = `圖上有 ${boundShapes(message.xml).length} 個綁定的形狀`
      }
      // 排版跑完會改動內容，於是送這個事件過來——但 `load` 自己也會送一次，
      // 而且比排版早到。所以等安靜下來才縮放。
      if (waiting.value) fitWhenSettled()
      break
  }
}

watch(() => [store.snapshot, environment.value?.id], () => loadModel())

// 換篩選只重塗，不重畫——重畫會洗掉手工排好的版面。
watch(() => [picked.value, store.onlyProblems], () => repaint(), { deep: true })

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
        <button class="filter" :class="{ on: filtering }" @click="filterOpen = !filterOpen">
          篩選<span v-if="filtering" class="count">{{ litCount }}/{{ totalShapes }}</span>
        </button>
        <select v-if="store.environments.length > 1" :value="environment.id" @change="chosen = ($event.target as HTMLSelectElement).value">
          <option v-for="e in store.environments" :key="e.id" :value="e.id">{{ e.slug }}</option>
        </select>
        <span v-else class="muted">{{ environment.slug }}</span>
        <span class="muted small">{{ links.length }} 條線</span>
        <!-- 萬用字元是 N×M，所以「幾條連線」跟「圖上幾條線」差很多。
             不講的話使用者只會覺得圖突然變成一團毛線，不知道為什麼。 -->
        <span v-if="tooMany" class="warn small">
          太多了，這張圖已經讀不動——篩選功能還沒做
        </span>
        <span class="grow" />
        <span class="muted small">{{ status }}</span>
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
          <!-- 空的部署圖跟壞掉的編輯器長得一樣，都是一片空白格線。
               不講的話使用者會以為是工具壞了。 -->
          <p v-if="empty" class="hint">
            <strong>{{ environment.slug }} 還沒有任何機器。</strong>
            部署圖畫的是「東西跑在哪裡」，所以要先到<strong>資源</strong>那一頁
            建機器與落地，這裡才畫得出東西。
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
