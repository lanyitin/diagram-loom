<script setup lang="ts">
/**
 * 資源檢視：每種資源一張表。
 *
 * # 為什麼需要這個檢視
 *
 * 覆蓋矩陣與連線表都只看得到**環境層的連線**。邏輯層（服務、契約、接點定義）
 * 在畫面上完全沒有出口——而那正是從零開始時第一個要建的東西。
 *
 * 更關鍵的是：**空專案的 lint 是空的**。沒有服務就沒有 L001，沒有契約就沒有
 * L002。lint 面板那條「修補迴圈」在空專案上根本啟動不了，所以一定要有
 * 一個地方能無中生有。
 *
 * # 欄位是 Rust 給的
 *
 * 「這個服務在各環境幾台」要走一次落地比對才算得出來，那是模型知識。
 * 這裡收到的是**已經算好的字串格子**，只負責畫成表格。
 *
 * 代價是不能在前端排序或篩選。等真的需要再說——先讓人建得出東西比較重要。
 */
import { computed, ref, watch } from 'vue'
import { commands } from '../lib/bindings'
import { useProject } from '../lib/store'
import type { ResourceRow, Table } from '../lib/model'

const store = useProject()

const tables = ref<Table[]>([])
const 目前分頁 = ref(0)

/** 環境層的表要看哪個環境。只有一個環境時不必問。 */
const 看哪個環境 = ref<string | null>(null)

const 現在這張 = computed<Table | null>(() => tables.value[目前分頁.value] ?? null)

async function 重新取() {
  if (!store.已開啟) return
  const env = 看哪個環境.value ?? store.環境[0]?.id ?? null
  const 回應 = await commands.resourceTables(env)
  if (回應.status === 'ok') {
    tables.value = 回應.data
    if (目前分頁.value >= tables.value.length) 目前分頁.value = 0
  } else {
    store.錯誤 = (回應.error as { message?: string })?.message ?? String(回應.error)
  }
}

// 每次專案變動都重算。表格上的數字（幾台、幾段）就是 lint 在看的同一批資料，
// 兩者不同步的話使用者會以為 lint 誤報。
watch(() => store.snapshot, () => void 重新取(), { immediate: true })
watch(看哪個環境, () => void 重新取())

async function 新增() {
  const t = 現在這張.value
  if (!t) return
  const 回應 = await commands.blankResource(t.kind, t.environment, null)
  if (回應.status !== 'ok') {
    store.錯誤 = (回應.error as { message?: string })?.message ?? String(回應.error)
    return
  }
  store.編輯資源中 = { resource: 回應.data, 新的: true, kind: t.title }
}

function 編輯(row: ResourceRow) {
  // 表格每一列都帶著自己的 `Resource`，表單直接拿它當起點。
  // 讓前端從 project 裡自己撈的話，要知道每種資源住在哪一層——那是模型知識。
  store.編輯資源中 = { resource: row.resource, 新的: false, kind: 現在這張.value?.title ?? '' }
}

function 想刪(row: ResourceRow) {
  store.刪除中 = {
    edit: { deleteResource: row.resource },
    kind: 現在這張.value?.title ?? '東西',
    label: row.cells[0] ?? row.id,
  }
}
</script>

<template>
  <div class="wrap">
    <div class="tabs">
      <button
        v-for="(t, i) in tables" :key="t.title"
        :class="{ on: 目前分頁 === i }"
        @click="目前分頁 = i"
      >
        {{ t.title }}
        <span class="n">{{ t.rows.length }}</span>
      </button>

      <span class="grow" />

      <!-- 環境層的表才需要問是哪個環境。邏輯層是母版，跟環境無關。 -->
      <label v-if="現在這張?.environment && store.環境.length > 1" class="pick">
        環境
        <select v-model="看哪個環境">
          <option v-for="e in store.環境" :key="e.id" :value="e.id">{{ e.slug }}</option>
        </select>
      </label>

      <button class="primary add" :disabled="store.忙碌中" @click="新增()">
        ＋ 新增{{ 現在這張?.title }}
      </button>
    </div>

    <div v-if="現在這張" class="body">
      <!-- 空的表不留白。第一次用的人需要知道「這是什麼、為什麼需要它」。 -->
      <p v-if="現在這張.rows.length === 0" class="empty muted">{{ 現在這張.emptyHint }}</p>

      <table v-else>
        <thead>
          <tr>
            <th class="sev" />
            <th v-for="c in 現在這張.columns" :key="c">{{ c }}</th>
            <th class="act" />
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in 現在這張.rows" :key="row.id">
            <td class="sev">
              <span v-if="row.severity" :class="['dot', row.severity]" :title="'這一列有 lint 問題'" />
            </td>
            <td
              v-for="(cell, i) in row.cells" :key="i"
              :class="{ mono: i === 0, muted: i > 0 }"
              :style="i === 0 && row.depth ? { paddingLeft: `${12 + row.depth * 18}px` } : undefined"
            >{{ cell }}</td>
            <td class="act">
              <button class="icon" :disabled="store.忙碌中" title="編輯" @click="編輯(row)">✎</button>
              <button class="icon del" :disabled="store.忙碌中" title="刪除" @click="想刪(row)">✕</button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.wrap { flex: 1; min-height: 0; display: flex; flex-direction: column; background: var(--surface); }

.tabs {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 12px;
  border-bottom: 1px solid var(--rule);
  background: var(--surface-2);
  flex-wrap: wrap;
}
.tabs > button {
  border: 0;
  background: transparent;
  color: var(--ink-3);
  padding: 3px 10px;
  border-radius: 5px;
  font-size: 12.5px;
}
.tabs > button:hover { background: var(--surface); }
.tabs > button.on {
  background: color-mix(in srgb, var(--warp) 14%, var(--surface));
  color: var(--ink);
  font-weight: 600;
}
.n { margin-left: 5px; font-size: 11px; color: var(--ink-3); font-variant-numeric: tabular-nums; }
.grow { flex: 1; }
.pick { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: var(--ink-2); }
.add { font-size: 12.5px; padding: 3px 10px; }

.body { flex: 1; min-height: 0; overflow: auto; }
.empty { padding: 40px 24px; text-align: center; max-width: 46ch; margin: 0 auto; line-height: 1.7; }

table { border-collapse: separate; border-spacing: 0; width: max-content; min-width: 100%; }
th, td {
  text-align: left;
  padding: 0 12px;
  height: var(--row);
  border-bottom: 1px solid var(--rule-2);
  white-space: nowrap;
}
thead th {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--surface-2);
  border-bottom: 1px solid var(--rule);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: .06em;
  color: var(--ink-3);
}
tbody tr:hover td { background: var(--surface-2); }

.sev { width: 26px; padding-right: 0; }
.dot { display: inline-block; width: 7px; height: 7px; border-radius: 50%; }
.dot.error { background: var(--broken); }
.dot.warning { background: var(--warn); }

.act { width: 62px; padding-left: 0; padding-right: 8px; text-align: right; }
.icon {
  padding: 0 6px;
  border-color: transparent;
  background: transparent;
  color: var(--ink-3);
  opacity: 0;
}
tbody tr:hover .icon, .icon:focus-visible { opacity: 1; }
.icon:hover { color: var(--ink); border-color: var(--rule); }
.del:hover { color: var(--broken); border-color: color-mix(in srgb, var(--broken) 45%, transparent); }
</style>
