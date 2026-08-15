<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { useProject } from './lib/store'
import { SCALES, apply as applyScale, load as loadScale, type Scale } from './lib/ui'
import CoverageMatrix from './components/CoverageMatrix.vue'
import DiagramView from './components/DiagramView.vue'
import AddConnection from './components/AddConnection.vue'
import AddInstances from './components/AddInstances.vue'
import AgentPanel from './components/AgentPanel.vue'
import ResourceForm from './components/ResourceForm.vue'
import ResourceView from './components/ResourceView.vue'
import CloseGuard from './components/CloseGuard.vue'
import DeleteConfirm from './components/DeleteConfirm.vue'
import EnvPicker from './components/EnvPicker.vue'
import ImportWizard from './components/ImportWizard.vue'
import LintPanel from './components/LintPanel.vue'

const store = useProject()

/**
 * 介面大小。放在標頭而不是某個設定頁裡——看不清楚的人第一件事就是找它，
 * 而藏在兩層選單後面的無障礙設定等於沒有。
 */
const scale = ref<Scale>(loadScale())
watch(scale, applyScale, { immediate: true })

/**
 * 開一個專案到這扇視窗。
 *
 * # 為什麼要先問未儲存
 *
 * 這會**取代**這扇視窗現在看的東西。以前沒有問，因為「開啟專案」是開機時
 * 做一次的事；多視窗之後它變成日常操作，而沒問就蓋掉等於安靜地丟掉工作。
 *
 * 要同時看兩個專案請按「新視窗」——那條路不會動到任何現有的東西。
 */
async function pickProject() {
  const picked = await open({ directory: true, title: '選擇專案資料夾（.loom）' })
  if (typeof picked !== 'string') return
  if (store.dirty) {
    replacing.value = picked
    return
  }
  await store.open(picked)
}

/** 使用者選好了、但這扇視窗還有未儲存的變更，等他決定。 */
const replacing = ref<string | null>(null)

async function saveThenOpen() {
  await store.save()
  // 存檔失敗就停在原地。開下去的話錯誤訊息會跟著被取代的專案一起消失。
  if (store.error) return
  const path = replacing.value
  replacing.value = null
  if (path) await store.open(path)
}

async function discardThenOpen() {
  const path = replacing.value
  replacing.value = null
  if (path) await store.open(path)
}

/**
 * 開一個全新的專案。
 *
 * 之前只能「開啟現有專案」——但使用者不見得有一份 Excel 可以匯，
 * 「從零開始」是真實情境。資料夾必須是空的，那是 Rust 擋的：
 * 選到一個已經有東西的資料夾就會蓋掉別人的檔案，而那沒辦法復原。
 */
async function newProject() {
  const picked = await open({ directory: true, title: '選一個空資料夾放新專案' })
  if (typeof picked !== 'string') return
  const name = picked.split('/').pop()?.replace(/\.loom$/, '') || '新專案'
  await store.createProject(picked, name)
}

/**
 * 系統選單按下去了。
 *
 * # 為什麼快捷鍵不再由這裡的 keydown 處理
 *
 * 原本 `⌘Z`／`⌘S` 是掛在主文件 `window` 上的監聽。當初拿掉是因為 draw.io
 * 跑在 `drawio://` 那個**跨 origin 的 iframe** 裡，keydown 不會冒泡給父文件
 * ——焦點在畫布裡時 `⌘S` 完全沒有存到東西，而畫面上沒有任何跡象。
 *
 * ⚠️ **那個 iframe 已經不在了**（畫布改成自己跑 maxGraph），但這個決定不變，
 * 理由換成兩個：原生選單的 accelerator 由系統派送，不管焦點在哪都會到；
 * 而畫布自己在 document 上聽鍵盤，這裡再聽一份就會變成兩個人搶同一個組合鍵。
 *
 * # 動作留在這裡，選單只負責「按下去了」
 *
 * 開檔對話框、未儲存確認、匯入精靈本來就都在前端。Rust 那邊再做一次的話，
 * 「開啟專案之前要問未儲存」就會有兩份，而且遲早只有一份被改到。
 */
const MENU: Record<string, () => void> = {
  'new-project': () => void newProject(),
  'open-project': () => void pickProject(),
  'new-window': () => void store.newWindow(),
  import: () => { store.importing = true },
  save: () => void store.save(),
  undo: () => void store.undo(),
  redo: () => void store.redo(),
  recheck: () => void store.recheck(),
  agent: () => { store.agentPanelOpen = true },
}

/**
 * AI Agent 改了東西之後把畫面拉新。
 *
 * 少了這個，Agent 做的事使用者**完全看不到**：畫面上還是它動手之前
 * 那份快照，連復原按鈕都是灰的（那份快照裡沒有可復原的步驟）。
 *
 * 而「看得到它在改什麼」正是把 MCP 掛在 App 裡、而不是做成獨立程序的
 * 全部理由——看不到的話，等他發現時已經是一整批改完了。
 */
let unlisten: (() => void) | null = null

let unlistenMenu: (() => void) | null = null

onMounted(async () => {
  unlistenMenu = await listen<string>('loom://menu', (e) => MENU[e.payload]?.())
  unlisten = await listen('loom://changed', () => void store.recheck())
  // 端點可能是自動啟用的，所以一開始就要問一次真正的狀態。
  await store.refreshAgent()
})

onUnmounted(() => {
  unlisten?.()
  unlistenMenu?.()
})
</script>

<template>
  <div class="app">
    <header>
      <strong v-if="store.isOpen">{{ store.snapshot!.project.name }}</strong>
      <!-- 還沒開專案時畫面上要有一條看得見的路。選單列不是每個人第一件事
           就會去看的地方，而一扇空視窗沒有別的線索。開了之後這兩顆就消失，
           日常操作走「檔案」選單。 -->
      <template v-else>
        <span class="muted">尚未開啟專案</span>
        <button :disabled="store.busy" @click="pickProject">開啟專案…</button>
        <button :disabled="store.busy" @click="newProject">新專案…</button>
      </template>
      <!-- 未儲存要看得出來，但不必用紅字嚇人——這個工具本來就是拿來一直改的。 -->
      <span v-if="store.dirty" class="dirty" title="有未儲存的變更">未儲存</span>
      <span v-if="store.isOpen" class="muted mono path">{{ store.snapshot!.root }}</span>

      <span class="grow" />

      <!-- 第一層：模式。整個介面唯一有實心底的分段控制項——它是持續的狀態，
           而旁邊的復原／重做是按一下就結束的動作，兩者長得不一樣才不會混。
           擺在復原／重做左邊，讀起來是「我在哪裡 → 我剛做了什麼」。 -->
      <div v-if="store.isOpen" class="mode" role="group" aria-label="模式">
        <button
          v-for="m in (['總攬', '圖'] as const)" :key="m"
          :aria-pressed="store.mode === m"
          @click="store.mode = m"
        >{{ m }}</button>
      </div>
      <span v-if="store.isOpen" class="divider" />

      <!-- 復原／重做搬進「編輯」選單了。那裡的項目直接寫「復原 新增服務」，
           比這兩顆按鈕的 tooltip 更說得出剛剛做了什麼，而且不必把滑鼠
           移過去停住才看得到。 -->

      <div class="seg scale" role="group" aria-label="介面大小">
        <button
          v-for="s in SCALES" :key="s.value"
          :class="{ on: scale === s.value }"
          :title="`介面大小：${s.label}`"
          @click="scale = s.value"
        >{{ s.label }}</button>
      </div>

      <!-- 新專案／開啟／新視窗／匯入都搬進「檔案」選單了。它們是每次只做
           一下、做完就離開的動作，擠在標頭上只是佔掉每天都在看的表格的位置。 -->
      <button :disabled="!store.isOpen || store.busy" @click="store.recheck()">重新檢查</button>
      <!-- 端點是開是關要在標頭看得出來。使用者踩過一次：重開 App 之後
           它預設是關的，而畫面上沒有任何地方講這件事，只能點進面板才知道。 -->
      <button
        :disabled="!store.isOpen || store.busy"
        :title="store.agentRunning ? 'AI 助手的端點開著' : 'AI 助手的端點是關的'"
        @click="store.agentPanelOpen = true"
      >
        <span :class="['lamp', { on: store.agentRunning }]" />
        AI 助手…
      </button>
      <button class="primary" :disabled="!store.dirty || store.busy" @click="store.save()">
        儲存
      </button>
    </header>

    <p v-if="store.error" class="failure" role="alert">{{ store.error }}</p>
    <!-- 不是錯誤，只是一句話。用紅字說「那個已經開在別的視窗」會像是
         使用者做錯了什麼，而他只是選了一個他已經在看的東西。 -->
    <p v-if="store.notice" class="notice-bar" role="status">
      {{ store.notice }}
      <button class="x" title="知道了" @click="store.notice = null">✕</button>
    </p>

    <!-- 取代這扇視窗現在看的專案之前先問。多視窗之後這變成日常操作，
         沒問就蓋掉等於安靜地丟掉工作。 -->
    <div v-if="replacing" class="scrim" @click.self="replacing = null">
      <section class="ask" role="dialog" aria-modal="true" aria-label="有未儲存的變更">
        <h2>有還沒存的變更</h2>
        <p class="muted">
          開別的專案會取代這扇視窗現在看的東西。要同時看兩個的話，
          按「取消」再用<strong>新視窗</strong>。
        </p>
        <footer>
          <button @click="replacing = null">取消</button>
          <span class="grow" />
          <button @click="discardThenOpen()">不存直接開</button>
          <button class="primary" @click="saveThenOpen()">存了再開</button>
        </footer>
      </section>
    </div>

    <template v-if="store.isOpen">
      <!--
        工具列只屬於「總攬」。

        圖模式**不畫這一列**，因為 `DiagramView` 自己就有一條控制列
        （篩選、環境、幾條線、重畫），那些狀態全都是它的內部狀態。
        把它們拉上來只會多一層 props，而換來的是同一個東西兩個家。

        這也就是拆成兩層之後省掉的那塊補丁：以前三個檢視擠在同一排，
        所以得寫「如果在圖，就把搜尋與篩選藏起來」——圖上沒有「列」可以篩，
        留著只會讓人以為打了字圖會跟著變。
      -->
      <div v-if="store.mode === '總攬'" class="toolbar">
        <!-- 第二層：檢視。同一份資料的兩種排法，比模式安靜一階。 -->
        <div class="views" role="group" aria-label="檢視">
          <button
            v-for="v in (['覆蓋矩陣', '資源'] as const)" :key="v"
            :aria-pressed="store.view === v"
            @click="store.view = v"
          >{{ v }}</button>
        </div>

        <input v-model="store.search" type="search" :placeholder="store.view === '覆蓋矩陣' ? '搜尋契約或用途…' : '搜尋名稱、位址或用途…'">
        <label class="toggle">
          <input v-model="store.onlyProblems" type="checkbox">
          只看有問題
        </label>
        <EnvPicker />

        <!-- 聚焦是從 lint 面板點過來的暫時狀態。看不見的篩選會讓人以為
             表格漏了東西，所以它必須寫在畫面上，而且一鍵拿得掉。 -->
        <button v-if="store.focus && store.view === '資源'" class="focus" @click="store.focus = null">
          <span class="mono">{{ store.focus.label }}</span>
          <span class="x">✕</span>
        </button>

        <span class="grow" />
        <span class="muted count">
          <template v-if="store.view === '覆蓋矩陣'">
            {{ store.visibleRelationships.length }} / {{ store.relationships.length }} 條契約
          </template>
          <template v-else-if="store.resourceTab === '連線'">
            {{ store.visibleRows.length }} / {{ store.rows.length }} 條連線
          </template>
        </span>
      </div>

      <!-- 聚焦到一項邏輯層的發現時，連線表本來就不會有列。
           留一片空白會讓人以為工具壞了，所以講清楚。 -->
      <p
        v-if="store.mode === '總攬' && store.focus && store.view === '資源'
          && store.resourceTab === '連線' && store.visibleRows.length === 0"
        class="notice"
      >
        這一項不對應到任何一條實際連線——它是邏輯層的問題，或是那個元素根本沒被任何連線碰到。
      </p>

      <DiagramView v-if="store.mode === '圖'" />
      <CoverageMatrix v-else-if="store.view === '覆蓋矩陣'" />
      <ResourceView v-else />

      <LintPanel />
    </template>

    <!-- 空狀態。第一次開啟時畫面不該是一片白。
         這個 v-else 必須緊貼著上面的 v-if——中間插任何東西都會把
         if/else 鏈打斷，變成兩個畫面同時出現。 -->
    <section v-else class="welcome">
      <h1>diagram-loom</h1>
      <p class="muted">
        管理系統的部署與連線，找出每個環境還缺什麼。
      </p>
      <div class="two">
        <button class="primary" @click="newProject">從零開始…</button>
        <button @click="pickProject">開啟現有專案…</button>
      </div>
      <p class="muted hint mono">fixtures/sample.loom 是一份刻意留了破洞的範例</p>
    </section>

    <ImportWizard v-if="store.importing" />
    <AddConnection />
    <AddInstances />
    <AgentPanel />
    <ResourceForm />
    <DeleteConfirm />
    <CloseGuard />
  </div>
</template>

<style scoped>
.app { height: 100%; display: flex; flex-direction: column; }
.grow { flex: 1; }

header, .toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 14px;
  flex-wrap: wrap;
}

header {
  background: var(--surface-2);
  border-bottom: 1px solid var(--rule);
}
.path { font-size: 11.5px; }

/* 三個字的級距鈕，比其他鈕窄。 */
.scale button { padding: 4px 9px; }

/* 端點開著時亮起來。灰的圈圈＝關著，實心＝開著——
   狀態不能只靠顏色，所以實心與空心也要不一樣。 */
.lamp {
  display: inline-block;
  width: 7px;
  height: 7px;
  margin-right: 6px;
  border-radius: 50%;
  border: 1px solid var(--ink-3);
}
.lamp.on { background: var(--ok); border-color: var(--ok); }

.dirty {
  font-size: 11px;
  padding: 1px 7px;
  border-radius: 9px;
  border: 1px solid color-mix(in srgb, var(--warn) 55%, transparent);
  color: var(--warn);
}

.toolbar {
  background: var(--surface);
  border-bottom: 1px solid var(--rule);
}
.toolbar input[type="search"] { min-width: 220px; }

.toggle { display: inline-flex; align-items: center; gap: 6px; color: var(--ink-2); }
.toggle input { accent-color: var(--warp); }
.count { font-size: 12px; }

/*
 * 導覽的三層，一層比一層安靜。使用者不必記哪個是哪個，看粗細就知道自己在哪。
 *
 *   第一層 模式    `.mode`   實心底的膠囊         總攬 | 圖
 *   第二層 檢視    `.views`  底線分頁             覆蓋矩陣 | 資源
 *   第三層 種類    ResourceView 的 `.tabs`  安靜的文字鈕   系統 服務 契約…
 *
 * `.seg` 不在這個階梯上——它是**動作**的鈕群（復原／重做、介面大小），
 * 不是「我在哪裡」。所以它跟模式長得不一樣是刻意的。
 */
.mode { display: flex; gap: 2px; padding: 2px; border-radius: 6px; background: var(--ground); }
.mode button {
  border: 0;
  background: transparent;
  color: var(--ink-3);
  padding: 3px 14px;
  border-radius: 4px;
  font-weight: 600;
  font-size: 13px;
}
.mode button:hover:not([aria-pressed='true']) { background: color-mix(in srgb, var(--surface) 55%, transparent); }
.mode button[aria-pressed='true'] {
  background: var(--surface);
  color: var(--ink);
  box-shadow: 0 1px 2px var(--shadow);
}

/* 兩組鈕擠在一起會被看成一組四顆，所以中間要有一條線。 */
.divider { width: 1px; align-self: stretch; margin: 3px 2px; background: var(--rule); }

.views { display: flex; gap: 2px; align-self: stretch; margin: -8px 4px -8px 0; }
.views button {
  border: 0;
  border-radius: 0;
  border-bottom: 2px solid transparent;
  background: transparent;
  color: var(--ink-3);
  padding: 0 11px 2px;
}
.views button:hover { background: transparent; color: var(--ink); }
.views button[aria-pressed='true'] { color: var(--ink); font-weight: 600; border-bottom-color: var(--warp); }

/* 動作鈕群：復原／重做、介面大小。 */
.seg { display: flex; border: 1px solid var(--rule); border-radius: 5px; overflow: hidden; }
.seg button {
  border: 0;
  border-radius: 0;
  border-right: 1px solid var(--rule);
  background: var(--surface);
  color: var(--ink-3);
  padding: 4px 12px;
}
.seg button:last-child { border-right: 0; }
.seg button.on { background: color-mix(in srgb, var(--warp) 12%, var(--surface)); color: var(--ink); font-weight: 600; }

/* 從 lint 面板跳過來的聚焦。看得見、按一下就沒。 */
.focus {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  max-width: 42ch;
  padding: 2px 8px;
  font-size: 11.5px;
  border-color: color-mix(in srgb, var(--warp) 45%, transparent);
  background: color-mix(in srgb, var(--warp) 10%, var(--surface));
}
.focus .mono { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.focus .x { color: var(--ink-3); }
.focus:hover .x { color: var(--ink); }

.notice {
  margin: 0;
  padding: 10px 14px;
  border-bottom: 1px solid var(--rule);
  background: var(--surface-2);
  color: var(--ink-2);
  font-size: 12.5px;
}

.failure {
  margin: 0;
  padding: 8px 14px;
  background: color-mix(in srgb, var(--broken) 12%, transparent);
  border-bottom: 1px solid color-mix(in srgb, var(--broken) 35%, transparent);
  color: var(--broken);
  user-select: text;
}

/* 不是錯誤，所以不是紅的。 */
.notice-bar {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0;
  padding: 7px 14px;
  background: color-mix(in srgb, var(--warp) 8%, transparent);
  border-bottom: 1px solid color-mix(in srgb, var(--warp) 30%, transparent);
  color: var(--ink-2);
  font-size: 13px;
}
.notice-bar .x {
  margin-left: auto;
  padding: 0 7px;
  border-color: transparent;
  background: transparent;
  color: var(--ink-3);
}

.scrim {
  position: fixed;
  inset: 0;
  z-index: 30;
  display: grid;
  place-items: center;
  background: color-mix(in srgb, #000 42%, transparent);
}
.ask {
  width: min(430px, 92%);
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 18px 20px 14px;
  border: 1px solid var(--rule);
  border-radius: 8px;
  background: var(--raise);
  box-shadow: 0 14px 40px var(--shadow);
}
.ask h2 { margin: 0; font-size: 15px; font-weight: 600; }
.ask p { margin: 0; font-size: 12.5px; line-height: 1.7; }
.ask footer { display: flex; align-items: center; gap: 8px; margin-top: 4px; }

.welcome {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
}
.welcome .two { display: flex; gap: 10px; }
.welcome h1 { margin: 0; font-size: 22px; font-weight: 600; letter-spacing: -.01em; }
.welcome p { margin: 0; }
.hint { font-size: 12px; }
</style>
