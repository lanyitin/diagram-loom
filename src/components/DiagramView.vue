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
import { boundShapes, toXml } from '../lib/diagram'
import type { Link } from '../lib/model'

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

/**
 * 一條萬用字元連線是 N×M 條線——12 台連 12 台就是 144 條。
 * 超過這個數字，圖已經沒有人讀得動了，該用篩選（尚未實作）而不是硬畫。
 */
const TOO_MANY = 600
const tooMany = computed(() => links.value.length > TOO_MANY)

/** 現在畫的是哪個環境。沒勾就畫第一個——一張圖只屬於一個環境。 */
const environment = computed(() => store.visibleEnvironments[0] ?? store.environments[0] ?? null)

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

  send({
    action: 'load',
    xml: toXml(store.snapshot.project, env, links.value),
    // 沒有 save 事件，所以靠 autosave 才知道使用者動了什麼。
    autosave: 1,
  })
  // 排版交給 draw.io 內建的 ELK。只有 layered 處理得了巢狀節點，
  // 而我們的圖天生巢狀（站點 → 機器 → 落地）——見 docs/nested-layout.md。
  send({ action: 'layout', layouts: 'verticalFlow' })
  send({ action: 'fit', maxScale: 1.2 })
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
        status.value = `圖上有 ${boundShapes(message.xml).length} 個綁定的形狀`
      }
      break
  }
}

watch(
  () => [store.snapshot, environment.value?.id],
  () => loadModel(),
)

// 掛在 mounted 而不是 iframe 的 load：load 每重載一次就多掛一個，
// 而多掛的那些不會壞掉，只會讓每則訊息被處理很多次——很難查。
onMounted(() => window.addEventListener('message', onMessage))
onUnmounted(() => window.removeEventListener('message', onMessage))
</script>

<template>
  <div class="wrap">
    <p v-if="!environment" class="empty muted">
      還沒有環境。一張部署圖畫的是「某個環境裡東西跑在哪」，所以要先建一個環境。
    </p>

    <template v-else>
      <div class="bar">
        <span class="muted">{{ environment.slug }}</span>
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
      <iframe
        ref="frame"
        class="editor"
        :src="`drawio://localhost/index.html?${PARAMS}`"
      />
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

.editor { flex: 1; min-height: 0; border: 0; width: 100%; }
.warn { color: var(--broken); }

.empty { padding: 40px 24px; text-align: center; max-width: 46ch; margin: 0 auto; line-height: 1.7; }
</style>
