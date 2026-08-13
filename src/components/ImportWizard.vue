<script setup lang="ts">
/**
 * Excel／CSV 匯入：選檔 → 預覽 → 套用。
 *
 * # 為什麼中間那步不能跳過
 *
 * 匯入是唯一會**大量**改動資料的操作。欄位貼錯一格，整個環境就被洗掉，
 * 而且不會馬上發現——使用者會以為工具算錯了。
 *
 * 所以沒有「直接匯入」的捷徑。預覽是流程的一部分，不是選項。
 *
 * # 步驟一其實不問環境
 *
 * wireframe 原本寫「選檔與環境」，但環境是試算表裡的欄位。
 * 讓使用者另外選一個，只會製造「檔案說 prod、使用者選 test，誰贏」的問題。
 * 改成由預覽告訴他這份檔案會動到哪些環境。
 */
import { computed, ref } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { commands } from '../lib/bindings'
import { useProject } from '../lib/store'
import type { Change, Element, Plan } from '../lib/model'

const store = useProject()

const 檔案 = ref<string | null>(null)
const 計畫 = ref<Plan | null>(null)
const 忙 = ref(false)
const 錯 = ref<string | null>(null)

const 種類名: Record<Element, string> = {
  environment: '環境',
  container: '服務（邏輯層）',
  endpointDef: '接點定義',
  relationship: '連線契約',
  deploymentNode: '機器',
  containerInstance: '服務落地',
  address: '位址',
  infrastructureNode: '設備',
  softwareSystemInstance: '外部系統落地',
  connection: '連線',
}

const 新增 = computed(() => 計畫.value?.changes.filter((c) => c.kind === 'added') ?? [])
const 更新 = computed(() => 計畫.value?.changes.filter((c) => c.kind === 'updated') ?? [])

/** 同一種類的變更收成一組，幾百筆時才讀得下去。 */
function 分組(changes: Change[]) {
  const g = new Map<Element, Change[]>()
  for (const c of changes) {
    if (!g.has(c.element)) g.set(c.element, [])
    g.get(c.element)!.push(c)
  }
  return [...g.entries()]
}

async function 選檔() {
  const 選了 = await open({
    title: '選擇要匯入的試算表',
    filters: [{ name: '試算表', extensions: ['csv', 'xlsx', 'xls', 'xlsm'] }],
  })
  if (typeof 選了 !== 'string') return

  檔案.value = 選了
  忙.value = true
  錯.value = null
  計畫.value = null
  try {
    const r = await commands.previewImport(選了)
    if (r.status === 'ok') 計畫.value = r.data
    else 錯.value = (r.error as { message?: string })?.message ?? String(r.error)
  } finally {
    忙.value = false
  }
}

async function 套用() {
  忙.value = true
  try {
    const r = await commands.applyImport()
    if (r.status === 'ok') {
      store.snapshot = r.data
      關閉()
    } else {
      錯.value = (r.error as { message?: string })?.message ?? String(r.error)
    }
  } finally {
    忙.value = false
  }
}

function 關閉() {
  commands.cancelImport()
  store.匯入中 = false
  檔案.value = null
  計畫.value = null
  錯.value = null
}

const 步驟 = computed(() => (計畫.value ? 2 : 1))
</script>

<template>
  <div class="backdrop" @click.self="關閉">
    <section class="dialog" role="dialog" aria-label="匯入試算表">
      <header>
        <strong>從試算表匯入</strong>
        <span class="grow" />
        <span class="steps mono">
          <span :class="{ on: 步驟 === 1 }">① 選檔</span>
          <span :class="{ on: 步驟 === 2 }">② 預覽差異</span>
          <span>③ 套用</span>
        </span>
      </header>

      <p v-if="錯" class="failure" role="alert">{{ 錯 }}</p>

      <!-- ① 選檔 -->
      <div v-if="!計畫" class="pick">
        <p class="muted">
          一列一條連線。環境寫在 <code>environment</code> 欄位裡，不必另外選。
        </p>
        <button class="primary" :disabled="忙" @click="選檔">
          {{ 忙 ? '讀取中…' : '選擇檔案…' }}
        </button>
        <p v-if="檔案" class="mono muted small">{{ 檔案 }}</p>
      </div>

      <!-- ② 預覽 -->
      <div v-else class="preview">
        <div class="summary">
          <span class="tally add">{{ 新增.length }} 新增</span>
          <span class="tally upd">{{ 更新.length }} 更新</span>
          <span class="tally same muted">{{ 計畫.unchanged }} 沒有變化</span>
          <span class="grow" />
          <span class="muted">動到：<b class="mono">{{ 計畫.environments.join('、') }}</b></span>
        </div>

        <!-- 警告要顯眼。位址不一致時工具保留舊的——不講清楚，
             使用者會以為試算表上的新 IP 已經寫進去了。 -->
        <div v-if="計畫.warnings.length" class="warnings">
          <b>{{ 計畫.warnings.length }} 項提醒（不會中斷匯入）</b>
          <ul>
            <li v-for="(w, i) in 計畫.warnings" :key="i">{{ w }}</li>
          </ul>
        </div>

        <div class="changes">
          <p v-if="計畫.changes.length === 0" class="muted nothing">
            這份檔案的內容跟現有資料完全相同，套用不會改變任何東西。
          </p>

          <template v-for="[element, list] in 分組(更新)" :key="'u' + element">
            <h4><span class="pip upd" />{{ 種類名[element] }}　<span class="muted">{{ list.length }} 項更新</span></h4>
            <table>
              <tbody>
                <tr v-for="c in list" :key="c.label">
                  <td class="mono name">{{ c.label }}</td>
                  <td class="mono was">{{ c.before }}</td>
                  <td class="arrow muted">→</td>
                  <td class="mono now">{{ c.after }}</td>
                </tr>
              </tbody>
            </table>
          </template>

          <template v-for="[element, list] in 分組(新增)" :key="'a' + element">
            <h4><span class="pip add" />{{ 種類名[element] }}　<span class="muted">{{ list.length }} 項新增</span></h4>
            <table>
              <tbody>
                <tr v-for="c in list" :key="c.label">
                  <td class="mono name">{{ c.label }}</td>
                  <td class="mono muted" colspan="3">{{ c.after }}</td>
                </tr>
              </tbody>
            </table>
          </template>
        </div>
      </div>

      <footer>
        <span v-if="計畫" class="muted small">套用後會重新檢查</span>
        <span class="grow" />
        <button @click="關閉">取消</button>
        <button
          v-if="計畫"
          class="primary"
          :disabled="忙 || 計畫.changes.length === 0"
          @click="套用"
        >
          套用 {{ 計畫.changes.length }} 項變更
        </button>
      </footer>
    </section>
  </div>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  background: rgba(10, 16, 14, .45);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 32px;
  z-index: 10;
}

.dialog {
  display: flex;
  flex-direction: column;
  width: min(860px, 100%);
  max-height: 100%;
  background: var(--surface);
  border: 1px solid var(--rule);
  border-radius: 8px;
  box-shadow: 0 12px 48px rgba(0, 0, 0, .28);
  overflow: hidden;
}

header, footer, .summary {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 16px;
}
header { background: var(--surface-2); border-bottom: 1px solid var(--rule); }
footer { background: var(--surface-2); border-top: 1px solid var(--rule); }
.grow { flex: 1; }
.small { font-size: 12px; }

.steps { display: flex; gap: 12px; font-size: 11.5px; color: var(--ink-3); }
.steps .on { color: var(--warp); font-weight: 600; }

.pick { padding: 28px 16px; display: flex; flex-direction: column; align-items: center; gap: 12px; }
.pick p { margin: 0; text-align: center; }

.preview { display: flex; flex-direction: column; min-height: 0; }
.summary { border-bottom: 1px solid var(--rule-2); }
.tally { font-weight: 600; font-size: 13px; }
.tally.add { color: var(--ok); }
.tally.upd { color: var(--warn); }
.tally.same { font-weight: 400; }

.warnings {
  padding: 10px 16px;
  background: color-mix(in srgb, var(--warn) 10%, transparent);
  border-bottom: 1px solid color-mix(in srgb, var(--warn) 35%, transparent);
  font-size: 13px;
}
.warnings ul { margin: 4px 0 0; padding-left: 1.3em; color: var(--ink-2); }

.changes { overflow: auto; padding-bottom: 8px; }
.nothing { padding: 28px 16px; text-align: center; }

h4 {
  display: flex;
  align-items: center;
  gap: 7px;
  margin: 0;
  padding: 9px 16px 5px;
  font-size: 12px;
  font-weight: 600;
  position: sticky;
  top: 0;
  background: var(--surface);
}
.pip { width: 7px; height: 7px; border-radius: 2px; }
.pip.add { background: var(--ok); }
.pip.upd { background: var(--warn); }

table { border-collapse: collapse; width: 100%; font-size: 12.5px; }
td { padding: 3px 16px; border-bottom: 1px solid var(--rule-2); white-space: nowrap; }
.name { width: 40%; }
.was { color: var(--ink-3); text-decoration: line-through; }
.arrow { width: 20px; text-align: center; }
.now { color: var(--ink); }

.failure {
  margin: 0;
  padding: 9px 16px;
  background: color-mix(in srgb, var(--broken) 12%, transparent);
  border-bottom: 1px solid color-mix(in srgb, var(--broken) 35%, transparent);
  color: var(--broken);
  user-select: text;
}
</style>
