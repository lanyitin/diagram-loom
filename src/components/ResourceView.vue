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
 * 排序與篩選在前端（`lib/rows.ts`）：那是「使用者現在想怎麼看」，不是模型知識。
 * 但**機器那張表是一棵樹**，所以兩者都不能直接 `.sort()` / `.filter()`——
 * 會把樹拆散或讓子節點變孤兒，而縮排還在，於是畫面顯示一個錯的從屬關係。
 *
 * # 連線也是一張表
 *
 * 連線本來是獨立的頂層檢視。但它跟其他資源一樣是「這個專案裡有什麼」，
 * 分兩個地方放只是讓人多記一件事。它的欄位跟資源不一樣（兩端、用途、
 * 備援），所以那一頁交給 [`ConnectionTable`] 畫。
 */
import { computed, ref, watch } from 'vue'
import { commands } from '../lib/bindings'
import { useProject } from '../lib/store'
import { filterRows, nextSort, sortRows, type Sort } from '../lib/rows'
import ConnectionTable from './ConnectionTable.vue'
import type { ResourceRow, Table } from '../lib/model'

const store = useProject()

/** 連線那一頁不是 `Table`，它的欄位長得不一樣。 */
const CONNECTIONS = '連線'

const tables = ref<Table[]>([])
// 空字串＝還沒載到表。`reload` 會挑第一張——**不是連線那一頁**：
// 從零開始的人要先建服務與契約，連線是後面的事。
const activeTab = ref<string>('')
const sort = ref<Sort | null>(null)

/** 分頁標題的清單。連線排在最後——它是環境層的東西。 */
const tabs = computed(() => [...tables.value.map((t) => t.title), CONNECTIONS])

/** 環境層的表要看哪個環境。只有一個環境時不必問。 */
const whichEnvironment = ref<string | null>(null)

const onConnections = computed(() => activeTab.value === CONNECTIONS)
const currentTable = computed<Table | null>(
  () => tables.value.find((t) => t.title === activeTab.value) ?? null,
)

/**
 * 要畫的列：先篩再排。
 *
 * 兩件事都走 `lib/rows.ts`，因為機器那張表是樹——直接 `.sort()` 會把樹拆散，
 * 直接 `.filter()` 會讓子節點變孤兒。
 */
const visibleRows = computed<ResourceRow[]>(() => {
  const t = currentTable.value
  if (!t) return []
  return sortRows(filterRows(t.rows, store.search), sort.value)
})

function clickHeader(column: number) {
  sort.value = nextSort(sort.value, column)
}

// 換一頁就把排序重設。欄位不一樣，沿用上一頁的第幾欄沒有意義。
watch(activeTab, () => { sort.value = null })

// lint 面板會指定跳到某一頁（例如「連線」）。
watch(() => store.resourceTab, (t) => { if (t) activeTab.value = t }, { immediate: true })

async function reload() {
  if (!store.isOpen) return
  const env = whichEnvironment.value ?? store.environments[0]?.id ?? null
  const res = await commands.resourceTables(env)
  if (res.status === 'ok') {
    tables.value = res.data
    if (!tabs.value.includes(activeTab.value)) activeTab.value = tabs.value[0] ?? CONNECTIONS
    store.resourceTab = activeTab.value
  } else {
    store.error = (res.error as { message?: string })?.message ?? String(res.error)
  }
}

// 每次專案變動都重算。表格上的數字（幾台、幾段）就是 lint 在看的同一批資料，
// 兩者不同步的話使用者會以為 lint 誤報。
watch(() => store.snapshot, () => void reload(), { immediate: true })
watch(whichEnvironment, () => void reload())

/** 新增連線要先問補給哪一條契約。空的話代表這個專案還沒有契約。 */
const pickingContract = ref(false)
const contract = ref<string>('')

function addConnection() {
  const env = whichEnvironment.value ?? store.environments[0]?.id
  if (!env || !contract.value) return
  const rel = store.relationships.find((r) => r.id === contract.value)
  pickingContract.value = false
  // 之後交給既有的那個對話框：Rust 會擬一份提案，使用者確認或換掉某一端。
  store.addingConnection = { environment: env, relationship: contract.value, label: rel?.slug ?? '' }
}

async function addNew() {
  if (onConnections.value) {
    contract.value = store.relationships[0]?.id ?? ''
    pickingContract.value = true
    return
  }
  const t = currentTable.value
  if (!t) return
  const res = await commands.blankResource(t.kind, t.environment, null)
  if (res.status !== 'ok') {
    store.error = (res.error as { message?: string })?.message ?? String(res.error)
    return
  }
  store.editingResource = { resource: res.data, isNew: true, kind: t.title }
}

function edit(row: ResourceRow) {
  // 表格每一列都帶著自己的 `Resource`，表單直接拿它當起點。
  // 讓前端從 project 裡自己撈的話，要知道每種資源住在哪一層——那是模型知識。
  store.editingResource = { resource: row.resource, isNew: false, kind: currentTable.value?.title ?? '' }
}

function askDelete(row: ResourceRow) {
  store.deleting = {
    edit: { deleteResource: row.resource },
    kind: currentTable.value?.title ?? '東西',
    label: row.cells[0] ?? row.id,
  }
}
</script>

<template>
  <div class="wrap">
    <div class="tabs">
      <button
        v-for="t in tables" :key="t.title"
        :class="{ on: activeTab === t.title }"
        @click="activeTab = t.title"
      >
        {{ t.title }}
        <span class="n">{{ t.rows.length }}</span>
      </button>
      <button :class="{ on: onConnections }" @click="activeTab = CONNECTIONS">
        {{ CONNECTIONS }}
        <span class="n">{{ store.visibleRows.length }}</span>
      </button>

      <span class="grow" />

      <!-- 環境層的表才需要問是哪個環境。邏輯層是母版，跟環境無關。 -->
      <label v-if="(currentTable?.environment || onConnections) && store.environments.length > 1" class="pick">
        環境
        <select v-model="whichEnvironment">
          <option v-for="e in store.environments" :key="e.id" :value="e.id">{{ e.slug }}</option>
        </select>
      </label>

      <button class="primary add" :disabled="store.busy" @click="addNew()">
        ＋ 新增{{ onConnections ? CONNECTIONS : currentTable?.title }}
      </button>
    </div>

    <!-- 新增連線要先問補給哪一條契約。連線一定屬於某條契約——
         沒有契約的連線 lint 會叫（L011），所以這裡不給「不選」這個選項。 -->
    <div v-if="pickingContract" class="scrim" @click.self="pickingContract = false">
      <section class="box" role="dialog" aria-modal="true">
        <h2>新增一條連線</h2>
        <p v-if="!store.relationships.length" class="muted lead">
          這個專案還沒有<strong>連線契約</strong>。契約是母版——先在「契約」那一頁
          寫下「誰要連誰」，才有東西可以在這個環境實現。
        </p>
        <template v-else>
          <p class="muted lead">
            這條連線是在實現哪一條契約？選好之後會擬一份提案給你確認。
          </p>
          <label class="field">
            <span>契約</span>
            <select v-model="contract">
              <option v-for="r in store.relationships" :key="r.id" :value="r.id">
                {{ r.slug }}
              </option>
            </select>
          </label>
        </template>
        <footer>
          <span class="grow" />
          <button @click="pickingContract = false">取消</button>
          <button v-if="store.relationships.length" class="primary" @click="addConnection()">下一步</button>
        </footer>
      </section>
    </div>

    <ConnectionTable v-if="onConnections" />

    <div v-else-if="currentTable" class="body">
      <!-- 空的表不留白。第一次用的人需要知道「這是什麼、為什麼需要它」。 -->
      <p v-if="currentTable.rows.length === 0" class="empty muted">{{ currentTable.emptyHint }}</p>
      <!-- 篩掉到一列都不剩跟「本來就是空的」是兩件事。混在一起的話，
           使用者會以為東西不見了。 -->
      <p v-else-if="visibleRows.length === 0" class="empty muted">
        「{{ store.search }}」在這一頁找不到東西。這一頁本來有
        {{ currentTable.rows.length }} 列。
      </p>

      <table v-else>
        <thead>
          <tr>
            <th class="sev" />
            <th
              v-for="(c, i) in currentTable.columns" :key="c"
              class="sortable" :aria-sort="sort?.column === i ? (sort.direction === 'asc' ? 'ascending' : 'descending') : 'none'"
              @click="clickHeader(i)"
            >{{ c }}</th>
            <th class="act" />
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in visibleRows" :key="row.id">
            <td class="sev">
              <span v-if="row.severity" :class="['dot', row.severity]" :title="'這一列有 lint 問題'" />
            </td>
            <td
              v-for="(cell, i) in row.cells" :key="i"
              :class="{ mono: i === 0, muted: i > 0 }"
              :style="i === 0 && row.depth ? { paddingLeft: `${12 + row.depth * 18}px` } : undefined"
            >{{ cell }}</td>
            <td class="act">
              <button class="icon" :disabled="store.busy" title="編輯" @click="edit(row)">✎</button>
              <button class="icon del" :disabled="store.busy" title="刪除" @click="askDelete(row)">✕</button>
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

th.sortable { cursor: pointer; user-select: none; white-space: nowrap; }
th.sortable:hover { color: var(--ink); }
/*
 * 箭頭用 CSS 畫，不放進標題的文字裡。
 *
 * 放進文字裡的話，欄位名稱就變成「名稱 ▴」——螢幕閱讀器會照唸，
 * 而排序狀態已經由 `aria-sort` 講過一次了。
 *
 * 一直佔著位置（透明的那個），不然排序時整排標題會左右跳。
 */
th.sortable::after {
  content: '▴';
  margin-left: 4px;
  font-size: 10px;
  opacity: 0;
}
th.sortable[aria-sort='ascending']::after { opacity: 0.75; }
th.sortable[aria-sort='descending']::after { content: '▾'; opacity: 0.75; }

.scrim {
  position: fixed;
  inset: 0;
  display: grid;
  place-items: center;
  background: color-mix(in srgb, #000 42%, transparent);
  z-index: 20;
}
.box {
  width: min(420px, 92vw);
  padding: 18px 20px 14px;
  border: 1px solid var(--rule);
  border-radius: 8px;
  background: var(--surface);
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.box h2 { margin: 0; font-size: 15px; font-weight: 600; }
.box p { margin: 0; }
.lead { font-size: 12.5px; line-height: 1.7; }
.field { display: flex; align-items: center; gap: 10px; font-size: 13px; }
.field > span { width: 3em; flex: none; color: var(--ink-2); }
.field select { flex: 1; }
.box footer { display: flex; align-items: center; gap: 8px; margin-top: 4px; }
.box footer .grow { flex: 1; }

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
