<script setup lang="ts">
import { open } from '@tauri-apps/plugin-dialog'
import { useProject } from './lib/store'
import CoverageMatrix from './components/CoverageMatrix.vue'
import EnvPicker from './components/EnvPicker.vue'

const store = useProject()

async function 選專案() {
  const 選了 = await open({ directory: true, title: '選擇專案資料夾（.loom）' })
  if (typeof 選了 === 'string') await store.開啟(選了)
}
</script>

<template>
  <div class="app">
    <header>
      <strong v-if="store.已開啟">{{ store.snapshot!.project.name }}</strong>
      <span v-else class="muted">尚未開啟專案</span>
      <span v-if="store.已開啟" class="muted mono path">{{ store.snapshot!.root }}</span>

      <span class="grow" />

      <button :disabled="store.忙碌中" @click="選專案">開啟專案…</button>
      <button :disabled="!store.已開啟 || store.忙碌中" @click="store.重新檢查()">重新檢查</button>
      <button class="primary" :disabled="!store.已開啟 || store.忙碌中" @click="store.儲存()">
        儲存
      </button>
    </header>

    <p v-if="store.錯誤" class="failure" role="alert">{{ store.錯誤 }}</p>

    <template v-if="store.已開啟">
      <div class="toolbar">
        <input v-model="store.搜尋" type="search" placeholder="搜尋契約或用途…">
        <label class="toggle">
          <input v-model="store.只看有問題" type="checkbox">
          只看有問題
        </label>
        <EnvPicker />
        <span class="grow" />
        <span class="muted count">{{ store.顯示的契約.length }} / {{ store.契約.length }} 條契約</span>
      </div>

      <CoverageMatrix />

      <footer>
        <span v-if="store.錯誤數" class="tally error">{{ store.錯誤數 }} 錯誤</span>
        <span v-if="store.警告數" class="tally warn">{{ store.警告數 }} 警告</span>
        <span v-if="!store.錯誤數 && !store.警告數" class="tally ok">沒有發現問題</span>
        <span class="grow" />
        <span class="muted">Lint 面板與連線表：階段 4 的下一步</span>
      </footer>
    </template>

    <!-- 空狀態。第一次開啟時畫面不該是一片白。 -->
    <section v-else class="welcome">
      <h1>diagram-loom</h1>
      <p class="muted">
        開啟一個專案資料夾，看看每個環境還缺什麼。
      </p>
      <button class="primary" @click="選專案">開啟專案…</button>
      <p class="muted hint mono">fixtures/sample.loom 是一份刻意留了破洞的範例</p>
    </section>
  </div>
</template>

<style scoped>
.app { height: 100%; display: flex; flex-direction: column; }
.grow { flex: 1; }

header, .toolbar, footer {
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

.toolbar {
  background: var(--surface);
  border-bottom: 1px solid var(--rule);
}
.toolbar input[type="search"] { min-width: 220px; }

.toggle { display: inline-flex; align-items: center; gap: 6px; color: var(--ink-2); }
.toggle input { accent-color: var(--warp); }
.count { font-size: 12px; }

footer {
  background: var(--surface-2);
  border-top: 1px solid var(--rule);
  font-size: 12.5px;
}

.tally { font-weight: 600; }
.tally.error { color: var(--broken); }
.tally.warn { color: var(--warn); }
.tally.ok { color: var(--ok); }

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
