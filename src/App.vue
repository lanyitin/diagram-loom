<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { useProject } from './lib/store'
import CoverageMatrix from './components/CoverageMatrix.vue'
import ConnectionTable from './components/ConnectionTable.vue'
import AddConnection from './components/AddConnection.vue'
import CloseGuard from './components/CloseGuard.vue'
import DeleteConfirm from './components/DeleteConfirm.vue'
import EnvPicker from './components/EnvPicker.vue'
import ImportWizard from './components/ImportWizard.vue'
import LintPanel from './components/LintPanel.vue'

const store = useProject()

async function 選專案() {
  const 選了 = await open({ directory: true, title: '選擇專案資料夾（.loom）' })
  if (typeof 選了 === 'string') await store.開啟(選了)
}

/**
 * Cmd+Z / Cmd+Shift+Z / Cmd+S。
 *
 * 復原若只有畫面上一顆按鈕，使用者不會相信它——他會改成「不敢亂按」。
 * 快捷鍵是「這個工具可以放心亂試」的訊號。
 */
function 按鍵(e: KeyboardEvent) {
  if (!(e.metaKey || e.ctrlKey) || !store.已開啟) return
  const 鍵 = e.key.toLowerCase()
  if (鍵 === 'z') {
    e.preventDefault()
    void (e.shiftKey ? store.重做() : store.復原())
  } else if (鍵 === 's') {
    e.preventDefault()
    void store.儲存()
  }
}

onMounted(() => window.addEventListener('keydown', 按鍵))
onUnmounted(() => window.removeEventListener('keydown', 按鍵))
</script>

<template>
  <div class="app">
    <header>
      <strong v-if="store.已開啟">{{ store.snapshot!.project.name }}</strong>
      <span v-else class="muted">尚未開啟專案</span>
      <!-- 未儲存要看得出來，但不必用紅字嚇人——這個工具本來就是拿來一直改的。 -->
      <span v-if="store.未儲存" class="dirty" title="有未儲存的變更">未儲存</span>
      <span v-if="store.已開啟" class="muted mono path">{{ store.snapshot!.root }}</span>

      <span class="grow" />

      <div v-if="store.已開啟" class="seg">
        <button
          :disabled="!store.可復原 || store.忙碌中"
          :title="store.可復原 ? `復原：${store.可復原}` : '沒有可以復原的動作'"
          @click="store.復原()"
        >↶ 復原</button>
        <button
          :disabled="!store.可重做 || store.忙碌中"
          :title="store.可重做 ? `重做：${store.可重做}` : '沒有可以重做的動作'"
          @click="store.重做()"
        >↷ 重做</button>
      </div>

      <button :disabled="store.忙碌中" @click="選專案">開啟專案…</button>
      <button :disabled="!store.已開啟 || store.忙碌中" @click="store.匯入中 = true">匯入試算表…</button>
      <button :disabled="!store.已開啟 || store.忙碌中" @click="store.重新檢查()">重新檢查</button>
      <button class="primary" :disabled="!store.未儲存 || store.忙碌中" @click="store.儲存()">
        儲存
      </button>
    </header>

    <p v-if="store.錯誤" class="failure" role="alert">{{ store.錯誤 }}</p>

    <template v-if="store.已開啟">
      <div class="toolbar">
        <div class="seg">
          <button
            v-for="v in (['覆蓋矩陣', '連線表'] as const)" :key="v"
            :class="{ on: store.檢視 === v }"
            @click="store.檢視 = v"
          >{{ v }}</button>
        </div>
        <input v-model="store.搜尋" type="search" :placeholder="store.檢視 === '覆蓋矩陣' ? '搜尋契約或用途…' : '搜尋契約、機器或位址…'">
        <label class="toggle">
          <input v-model="store.只看有問題" type="checkbox">
          只看有問題
        </label>
        <EnvPicker />

        <!-- 聚焦是從 lint 面板點過來的暫時狀態。看不見的篩選會讓人以為
             表格漏了東西，所以它必須寫在畫面上，而且一鍵拿得掉。 -->
        <button v-if="store.聚焦 && store.檢視 === '連線表'" class="focus" @click="store.聚焦 = null">
          <span class="mono">{{ store.聚焦.label }}</span>
          <span class="x">✕</span>
        </button>

        <span class="grow" />
        <span class="muted count">
          <template v-if="store.檢視 === '覆蓋矩陣'">
            {{ store.顯示的契約.length }} / {{ store.契約.length }} 條契約
          </template>
          <template v-else>
            {{ store.顯示的列.length }} / {{ store.列.length }} 條連線
          </template>
        </span>
      </div>

      <!-- 聚焦到一項邏輯層的發現時，連線表本來就不會有列。
           留一片空白會讓人以為工具壞了，所以講清楚。 -->
      <p v-if="store.聚焦 && store.檢視 === '連線表' && store.顯示的列.length === 0" class="notice">
        這一項不對應到任何一條實際連線——它是邏輯層的問題，或是那個元素根本沒被任何連線碰到。
      </p>

      <CoverageMatrix v-if="store.檢視 === '覆蓋矩陣'" />
      <ConnectionTable v-else />

      <LintPanel />
    </template>

    <!-- 空狀態。第一次開啟時畫面不該是一片白。
         這個 v-else 必須緊貼著上面的 v-if——中間插任何東西都會把
         if/else 鏈打斷，變成兩個畫面同時出現。 -->
    <section v-else class="welcome">
      <h1>diagram-loom</h1>
      <p class="muted">
        開啟一個專案資料夾，看看每個環境還缺什麼。
      </p>
      <button class="primary" @click="選專案">開啟專案…</button>
      <p class="muted hint mono">fixtures/sample.loom 是一份刻意留了破洞的範例</p>
    </section>

    <ImportWizard v-if="store.匯入中" />
    <AddConnection />
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
.welcome h1 { margin: 0; font-size: 22px; font-weight: 600; letter-spacing: -.01em; }
.welcome p { margin: 0; }
.hint { font-size: 12px; }
</style>
