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
  running: false, url: null, preferredPort: null, requireToken: true, autostart: false,
})
const config = ref<string | null>(null)
const copied = ref(false)

/** 埠的輸入框。空字串＝交給作業系統挑。 */
const portInput = ref('')

function take(next: McpStatus) {
  status.value = next
  portInput.value = next.preferredPort === null ? '' : String(next.preferredPort)
}

async function refresh() {
  const res = await commands.mcpStatus()
  if (res.status === 'ok') take(res.data)
  if (status.value.running) await reveal()
}

/** 改設定。端點在跑的話 Rust 那邊會重開——埠與 token 是啟動時決定的。 */
async function apply(next: Partial<{ port: number | null; requireToken: boolean; autostart: boolean }>) {
  const port = 'port' in next ? next.port! : parsePort()
  const res = await commands.setMcpConfig(
    port,
    next.requireToken ?? status.value.requireToken,
    next.autostart ?? status.value.autostart,
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
        <span>{{ status.running ? '已開啟' : '關閉中' }}</span>
      </label>

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
        <label class="check">
          <input
            type="checkbox" :checked="status.autostart"
            @change="apply({ autostart: ($event.target as HTMLInputElement).checked })"
          >
          開啟 App 時自動啟用
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
