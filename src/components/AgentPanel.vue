<script setup lang="ts">
/**
 * 讓 AI Agent 接進來。
 *
 * # 為什麼預設關閉，而且要按一下才看得到 token
 *
 * 打開之後，本機會多一個**可以改你檔案的端點**。那不是一個該安靜發生的事。
 *
 * token 每次啟用重新產生、不存檔——關掉 App 就該預期這個門關了。
 * 所以這裡也不「記住上次的設定」：下次要用就再打開一次。
 *
 * # Agent 改的東西不會自動存檔
 *
 * 它走的是跟畫面同一條復原鏈，所以你看得到、退得掉、要存才存。
 * 這句話寫在畫面上，因為那是使用者最需要先知道的事。
 */
import { ref, watch } from 'vue'
import { commands } from '../lib/bindings'
import { useProject } from '../lib/store'
import type { McpStatus } from '../lib/model'

const store = useProject()

const status = ref<McpStatus>({ running: false, url: null })
const config = ref<string | null>(null)
const copied = ref(false)

async function refresh() {
  const res = await commands.mcpStatus()
  if (res.status === 'ok') status.value = res.data
}

watch(() => store.agentPanelOpen, (open) => { if (open) void refresh() }, { immediate: true })

async function toggle() {
  const res = status.value.running ? await commands.stopMcp() : await commands.startMcp()
  if (res.status !== 'ok') {
    store.error = (res.error as { message?: string })?.message ?? String(res.error)
    return
  }
  status.value = res.data
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

      <template v-if="status.running">
        <p class="muted small">把這一段貼進 Agent 的 MCP 設定裡：</p>
        <pre class="mono config">{{ config ?? '取得中…' }}</pre>
        <div class="row">
          <button :disabled="!config" @click="copy()">{{ copied ? '已複製' : '複製設定' }}</button>
          <span class="muted small">{{ status.url }}</span>
        </div>
      </template>

      <!-- 這兩件事使用者一定要先知道，不能藏在文件裡。 -->
      <ul class="notes">
        <li><strong>不會自動存檔。</strong>Agent 改的東西會標成「未儲存」，你看過再決定；⌘Z 也退得掉。</li>
        <li><strong>只有這台機器連得到</strong>，而且需要上面那組 token。關掉 App 這個門就關了。</li>
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
