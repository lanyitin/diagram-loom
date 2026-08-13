<script setup lang="ts">
import { open } from '@tauri-apps/plugin-dialog'
import { useProject } from './lib/store'
import CoverageMatrix from './components/CoverageMatrix.vue'
import ConnectionTable from './components/ConnectionTable.vue'
import EnvPicker from './components/EnvPicker.vue'
import LintPanel from './components/LintPanel.vue'

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

      <CoverageMatrix v-if="store.檢視 === '覆蓋矩陣'" />
      <ConnectionTable v-else />

      <LintPanel />
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
