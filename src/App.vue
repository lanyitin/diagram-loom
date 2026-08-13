<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { useProject } from './lib/store'
import { SCALES, apply as applyScale, load as loadScale, type Scale } from './lib/ui'
import CoverageMatrix from './components/CoverageMatrix.vue'
import DiagramView from './components/DiagramView.vue'
import ConnectionTable from './components/ConnectionTable.vue'
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

async function pickProject() {
  const picked = await open({ directory: true, title: '選擇專案資料夾（.loom）' })
  if (typeof picked === 'string') await store.open(picked)
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
 * Cmd+Z / Cmd+Shift+Z / Cmd+S。
 *
 * 復原若只有畫面上一顆按鈕，使用者不會相信它——他會改成「不敢亂按」。
 * 快捷鍵是「這個工具可以放心亂試」的訊號。
 */
function onKeydown(e: KeyboardEvent) {
  if (!(e.metaKey || e.ctrlKey) || !store.isOpen) return
  const key = e.key.toLowerCase()
  if (key === 'z') {
    e.preventDefault()
    void (e.shiftKey ? store.redo() : store.undo())
  } else if (key === 's') {
    e.preventDefault()
    void store.save()
  }
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

onMounted(async () => {
  window.addEventListener('keydown', onKeydown)
  unlisten = await listen('loom://changed', () => void store.recheck())
  // 端點可能是自動啟用的，所以一開始就要問一次真正的狀態。
  await store.refreshAgent()
})

onUnmounted(() => {
  window.removeEventListener('keydown', onKeydown)
  unlisten?.()
})
</script>

<template>
  <div class="app">
    <header>
      <strong v-if="store.isOpen">{{ store.snapshot!.project.name }}</strong>
      <span v-else class="muted">尚未開啟專案</span>
      <!-- 未儲存要看得出來，但不必用紅字嚇人——這個工具本來就是拿來一直改的。 -->
      <span v-if="store.dirty" class="dirty" title="有未儲存的變更">未儲存</span>
      <span v-if="store.isOpen" class="muted mono path">{{ store.snapshot!.root }}</span>

      <span class="grow" />

      <div v-if="store.isOpen" class="seg">
        <button
          :disabled="!store.undoLabel || store.busy"
          :title="store.undoLabel ? `復原：${store.undoLabel}` : '沒有可以復原的動作'"
          @click="store.undo()"
        >↶ 復原</button>
        <button
          :disabled="!store.redoLabel || store.busy"
          :title="store.redoLabel ? `重做：${store.redoLabel}` : '沒有可以重做的動作'"
          @click="store.redo()"
        >↷ 重做</button>
      </div>

      <div class="seg scale" role="group" aria-label="介面大小">
        <button
          v-for="s in SCALES" :key="s.value"
          :class="{ on: scale === s.value }"
          :title="`介面大小：${s.label}`"
          @click="scale = s.value"
        >{{ s.label }}</button>
      </div>

      <button :disabled="store.busy" @click="newProject">新專案…</button>
      <button :disabled="store.busy" @click="pickProject">開啟專案…</button>
      <button :disabled="!store.isOpen || store.busy" @click="store.importing = true">匯入試算表…</button>
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

    <template v-if="store.isOpen">
      <div class="toolbar">
        <div class="seg">
          <button
            v-for="v in (['覆蓋矩陣', '連線表', '資源', '圖'] as const)" :key="v"
            :class="{ on: store.view === v }"
            @click="store.view = v"
          >{{ v }}</button>
        </div>
        <!-- 搜尋與篩選是表格的東西。圖上沒有「列」可以篩，留著只會讓人
             以為打了字圖會跟著變。 -->
        <template v-if="store.view !== '圖'">
          <input v-model="store.search" type="search" :placeholder="store.view === '覆蓋矩陣' ? '搜尋契約或用途…' : '搜尋契約、機器或位址…'">
          <label class="toggle">
            <input v-model="store.onlyProblems" type="checkbox">
            只看有問題
          </label>
        </template>
        <EnvPicker />

        <!-- 聚焦是從 lint 面板點過來的暫時狀態。看不見的篩選會讓人以為
             表格漏了東西，所以它必須寫在畫面上，而且一鍵拿得掉。 -->
        <button v-if="store.focus && store.view === '連線表'" class="focus" @click="store.focus = null">
          <span class="mono">{{ store.focus.label }}</span>
          <span class="x">✕</span>
        </button>

        <span class="grow" />
        <span class="muted count">
          <template v-if="store.view === '覆蓋矩陣'">
            {{ store.visibleRelationships.length }} / {{ store.relationships.length }} 條契約
          </template>
          <!-- 圖是一個環境一張，數「幾條連線」在這裡沒有意義。 -->
          <template v-else-if="store.view === '圖'">一張圖只畫一個環境</template>
          <template v-else>
            {{ store.visibleRows.length }} / {{ store.rows.length }} 條連線
          </template>
        </span>
      </div>

      <!-- 聚焦到一項邏輯層的發現時，連線表本來就不會有列。
           留一片空白會讓人以為工具壞了，所以講清楚。 -->
      <p v-if="store.focus && store.view === '連線表' && store.visibleRows.length === 0" class="notice">
        這一項不對應到任何一條實際連線——它是邏輯層的問題，或是那個元素根本沒被任何連線碰到。
      </p>

      <CoverageMatrix v-if="store.view === '覆蓋矩陣'" />
      <ConnectionTable v-else-if="store.view === '連線表'" />
      <DiagramView v-else-if="store.view === '圖'" />
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

/* 檢視切換。兩個檢視是同一份資料的兩種排法，所以用分段控制項而不是分頁。 */
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
