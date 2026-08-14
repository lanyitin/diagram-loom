<script setup lang="ts">
/**
 * 讓 AI Agent 接進來。
 *
 * # 設定是 App 層級的
 *
 * 埠、token、自動啟用都跟著**這台機器**，不跟著專案。理由是
 * Agent 那邊的設定裡只有一個 URL——跟著專案走的話，切一次專案
 * 那份設定就失效，而「不用每次重貼」正是這些設定存在的唯一理由。
 *
 * 端點本身也是 App 層級的：它同時服務所有開著的專案，Agent 用 `project`
 * 指名要動哪一個。所以「這個專案要不要開 MCP」在模型上不成立。
 *
 * # 開關就是偏好
 *
 * 這裡曾經有兩個控制項：一個開關，加一個「開啟 App 時自動啟用」。
 * 於是打開端點的人下次還要再打開一次——除非他發現了第二個方塊。
 * 現在開關自己記住（`mcp::remember`），畫面直說它記住了。
 *
 * # 三道鎖，只有一道可以關
 *
 * 綁 `127.0.0.1` 與擋 `Origin` 是無條件的。token 可以關掉，
 * 而畫面上要直說關掉之後少了什麼——不能讓人以為自己還有三道。
 */
import { ref, watch } from 'vue'
import { commands } from '../lib/bindings'
import { useProject } from '../lib/store'
import type { McpStatus } from '../lib/model'

const store = useProject()

const status = ref<McpStatus>({
  running: false, url: null, preferredPort: null, requireToken: true,
  autostart: false, autostartFailed: null,
})
const config = ref<string | null>(null)
const copied = ref(false)

/** 埠的輸入框。空字串＝交給作業系統挑。 */
const portInput = ref('')

function take(next: McpStatus) {
  status.value = next
  portInput.value = next.preferredPort === null ? '' : String(next.preferredPort)
  // 標頭那顆燈讀的是 store。兩邊各存一份的話遲早會各說各話。
  store.agentRunning = next.running
}

async function refresh() {
  const res = await commands.mcpStatus()
  if (res.status === 'ok') take(res.data)
  if (status.value.running) await reveal()
}

/** 改設定。端點在跑的話 Rust 那邊會重開——埠與 token 是啟動時決定的。 */
async function apply(next: Partial<{ port: number | null; requireToken: boolean }>) {
  const port = 'port' in next ? next.port! : parsePort()
  const res = await commands.setMcpConfig(
    port,
    next.requireToken ?? status.value.requireToken,
  )
  if (res.status !== 'ok') {
    store.error = (res.error as { message?: string })?.message ?? String(res.error)
    await refresh()
    return
  }
  take(res.data)
  copied.value = false
  if (status.value.running) await reveal()
}

function parsePort(): number | null {
  const n = Number(portInput.value.trim())
  return portInput.value.trim() === '' || !Number.isInteger(n) || n < 1 || n > 65535 ? null : n
}

async function regenerate() {
  const res = await commands.regenerateMcpToken()
  if (res.status !== 'ok') {
    store.error = (res.error as { message?: string })?.message ?? String(res.error)
    return
  }
  take(res.data)
  copied.value = false
  if (status.value.running) await reveal()
}

watch(() => store.agentPanelOpen, (open) => { if (open) void refresh() }, { immediate: true })

async function toggle() {
  const res = status.value.running ? await commands.stopMcp() : await commands.startMcp()
  if (res.status !== 'ok') {
    store.error = (res.error as { message?: string })?.message ?? String(res.error)
    return
  }
  take(res.data)
  config.value = null
  copied.value = false
  if (status.value.running) await reveal()
}

/** token 只從這裡出去一次。狀態查詢不帶它——狀態會被畫面到處傳。 */
async function reveal() {
  const res = await commands.mcpConfig()
  if (res.status === 'ok') config.value = res.data
}

async function copy() {
  if (!config.value) return
  await navigator.clipboard.writeText(config.value)
  copied.value = true
}
</script>

<template>
  <div v-if="store.agentPanelOpen" class="scrim" @click.self="store.agentPanelOpen = false">
    <section class="box" role="dialog" aria-modal="true">
      <h2>讓 AI 助手接進來</h2>
      <p class="muted lead">
        打開之後，AI Agent 可以讀這份專案、照你給的資料建立服務與連線，
        並且用 lint 自我檢查。
      </p>

      <label class="switch">
        <input type="checkbox" :checked="status.running" @change="toggle()">
        <span :class="{ off: !status.running }">
          {{ status.running ? '端點開著，Agent 連得進來' : '端點是關的，Agent 連不進來' }}
        </span>
      </label>
      <!-- 使用者踩過一次：設定都填好了、以為就能用，其實忘了打開這個開關。
           填好卻沒開的時候直說，不要只給一個中性的核取方塊。 -->
      <p v-if="!status.running && status.preferredPort !== null" class="muted hint">
        設定都在，但還沒啟用。勾上面那個就會開起來。
      </p>
      <!-- 這個開關**就是**偏好本身，旁邊不再放第二個「而且下次也要」的方塊。
           但要說出來——安靜地記住跟安靜地忘記一樣讓人不放心。 -->
      <p v-if="status.autostart && status.running" class="muted hint">
        記住了，下次開 App 會自動開起來。不想要的話把上面那個關掉。
      </p>
      <!-- 自動啟用失敗的話一定要說。不說的話畫面跟「忘了打開」長得一模一樣，
           而使用者會以為自己上次忘了勾——那正是這個偏好要消滅的東西。 -->
      <p v-if="status.autostartFailed" class="warn hint">
        <strong>這次沒有自動開起來。</strong>{{ status.autostartFailed }}
      </p>

      <div class="settings">
        <label class="field">
          <span>埠</span>
          <input
            v-model="portInput" type="text" inputmode="numeric" class="mono"
            placeholder="留空＝每次由系統挑" @change="apply({})"
          >
        </label>
        <p class="muted tip">
          固定一個埠，Agent 那邊的設定就不必每次重開都重貼。被別的程式佔用時會直接報錯，
          <strong>不會偷偷換一個</strong>——偷偷換的話你的設定會安靜地連到空氣。
        </p>

        <label class="check">
          <input
            type="checkbox" :checked="status.requireToken"
            @change="apply({ requireToken: ($event.target as HTMLInputElement).checked })"
          >
          需要 token
        </label>
      </div>

      <template v-if="status.running">
        <div class="row">
          <p class="muted small grow">把這一段貼進 Agent 的 MCP 設定裡：</p>
          <button v-if="status.requireToken" @click="regenerate()">換一組 token</button>
        </div>
        <pre class="mono config">{{ config ?? '取得中…' }}</pre>
        <div class="row">
          <button :disabled="!config" @click="copy()">{{ copied ? '已複製' : '複製設定' }}</button>
          <span class="muted small">{{ status.url }}</span>
        </div>
      </template>

      <!-- 這幾件事使用者一定要先知道，不能藏在文件裡。 -->
      <ul class="notes">
        <li><strong>不會自動存檔。</strong>Agent 改的東西會標成「未儲存」，你看過再決定；⌘Z 也退得掉。</li>
        <li><strong>只有這台機器連得到，而且瀏覽器裡的網頁打不到。</strong>這兩道關不掉。</li>
        <li v-if="!status.requireToken" class="warn">
          <strong>token 檢查是關的。</strong>這台機器上的<strong>任何</strong>程式都能改這份專案。
          單人使用的機器影響不大（那些程式本來就讀得到你的檔案），
          但共用的機器上別關。
        </li>
      </ul>

      <footer>
        <span class="grow" />
        <button @click="store.agentPanelOpen = false">關閉</button>
      </footer>
    </section>
  </div>
</template>

<style scoped>
.scrim {
  position: fixed;
  inset: 0;
  display: grid;
  place-items: center;
  background: color-mix(in srgb, #000 42%, transparent);
  z-index: 20;
}
.box {
  width: min(560px, 92vw);
  max-height: 86%;
  overflow: auto;
  padding: 18px 20px 14px;
  border: 1px solid var(--rule);
  border-radius: 8px;
  background: var(--surface);
  display: flex;
  flex-direction: column;
  gap: 12px;
}
h2 { margin: 0; font-size: 15px; font-weight: 600; }
p { margin: 0; }
.lead { font-size: 12.5px; line-height: 1.7; }
.small { font-size: 12px; }

.switch { display: inline-flex; align-items: center; gap: 8px; font-size: 13px; }
.switch input { accent-color: var(--warp); }
.switch .off { color: var(--ink-3); }
.hint { font-size: 11.5px; color: var(--ink-3); margin-left: 22px; }

.config {
  margin: 0;
  padding: 10px 12px;
  border: 1px solid var(--rule);
  border-radius: 5px;
  background: var(--surface-2);
  font-size: 11.5px;
  line-height: 1.6;
  overflow-x: auto;
  user-select: text;
}
.row { display: flex; align-items: center; gap: 10px; }
.grow { flex: 1; }

.settings { display: flex; flex-direction: column; gap: 8px; }
.field { display: flex; align-items: center; gap: 10px; font-size: 13px; }
.field > span { width: 2.5em; flex: none; color: var(--ink-2); }
.field input { width: 12em; }
.tip { font-size: 11.5px; line-height: 1.6; color: var(--ink-3); }
.check { display: inline-flex; align-items: center; gap: 6px; font-size: 13px; }
.check input { accent-color: var(--warp); }

.notes .warn { color: var(--broken); }

.notes {
  margin: 0;
  padding: 10px 12px;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 6px;
  border: 1px solid color-mix(in srgb, var(--warn) 40%, transparent);
  border-radius: 5px;
  background: color-mix(in srgb, var(--warn) 8%, transparent);
  font-size: 12px;
  line-height: 1.6;
  color: var(--ink-2);
}

footer { display: flex; align-items: center; gap: 8px; margin-top: 4px; }
.grow { flex: 1; }
</style>
