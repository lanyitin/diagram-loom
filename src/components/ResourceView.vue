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
 * 「這個服務在各環境幾台」要走一次實體比對才算得出來，那是模型知識。
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
import { computed, onUnmounted, ref, watch } from 'vue'
import { commands } from '../lib/bindings'
import { useProject } from '../lib/store'
import { filterRows, sortRows, type Sort } from '../lib/rows'
import { useColumns } from '../lib/columns'
import ConnectionTable from './ConnectionTable.vue'
import TableHead from './TableHead.vue'
import type { ResourceRow, Table, TableGroup } from '../lib/model'

const store = useProject()

/** 連線那一頁不是 `Table`，它的欄位長得不一樣。 */
const CONNECTIONS = '連線'

const tables = ref<Table[]>([])
// 空字串＝還沒載到表。`reload` 會挑第一張——**不是連線那一頁**：
// 從零開始的人要先建服務與契約，連線是後面的事。
const activeTab = ref<string>('')
const sort = ref<Sort | null>(null)

/** 分組標題。Rust 只給代號，中文在畫面這一層。 */
const GROUP_LABEL: Record<string, string> = {
  logical: '邏輯層 · 母版',
  environment: '環境層 · 分身',
}

/**
 * 分頁列上的一頁。
 *
 * `table` 為 `null` 的只有連線——它的欄位跟資源不一樣（兩端、用途、備援），
 * 所以那一頁交給 [`ConnectionTable`] 畫。
 */
interface Tab {
  title: string
  group: TableGroup
  count: number
  table: Table | null
}

/**
 * 分頁列。順序完全照 Rust 給的，這裡不重排。
 *
 * 「環境」那張表（`group: 'project'`）**不進分頁列**——它列的是全部環境，
 * 不隸屬於任何一個，而使用者要找它的時候會去按右邊的環境選單。
 * 它從 `tables` 拿得到，由「管理環境…」開出來。
 *
 * 連線接在最後面。它不是 Rust 給的 `Table`，但**位置沒有選擇餘地**：
 * 一條連線要兩端加上一條契約才開得了，所以它必然是環境層的最後一項。
 */
const tabs = computed<Tab[]>(() => [
  ...tables.value
    .filter((t) => t.group !== 'project')
    .map((t) => ({ title: t.title, group: t.group, count: t.rows.length, table: t })),
  {
    title: CONNECTIONS,
    group: 'environment' as TableGroup,
    count: store.visibleRows.length,
    table: null,
  },
])

/** 環境層的表要看哪個環境。只有一個環境時不必問。 */
const whichEnvironment = ref<string | null>(null)

const onConnections = computed(() => activeTab.value === CONNECTIONS)
const currentTable = computed<Table | null>(
  () => tables.value.find((t) => t.title === activeTab.value) ?? null,
)

/** 「環境」那張表。不在分頁列上，只在「管理環境…」那個對話框裡出現。 */
const environmentTable = computed<Table | null>(
  () => tables.value.find((t) => t.group === 'project') ?? null,
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
  return sortRows(filterRows(t.rows, store.search), sort.value, t.columns)
})

/**
 * 欄位偏好綁在 `kind` 上，不是分頁標題。
 *
 * 標題是給人看的字，哪天把「服務」改成別的用詞，使用者存下來的設定
 * 就會整份對不上——而畫面看起來完全正常，只是他調過的東西全沒了。
 */
const columnKey = computed(() => currentTable.value?.kind ?? '')
const { prefs, visible, toggle, setWidth, clearWidth } = useColumns(columnKey)
const columns = computed(() => visible(currentTable.value?.columns ?? []))

/** 一格在 `cells` 裡的第幾格。欄位可以藏之後，畫面順序不等於資料順序。 */
function cellOf(row: ResourceRow, column: string): string {
  return row.cells[currentTable.value?.columns.indexOf(column) ?? -1] ?? ''
}

const head = ref<InstanceType<typeof TableHead> | null>(null)

// 換一頁就把排序重設。欄位不一樣，沿用上一頁的欄名沒有意義。
watch(activeTab, () => { sort.value = null })

// lint 面板會指定跳到某一頁（例如「連線」）。
watch(() => store.resourceTab, (t) => { if (t) activeTab.value = t }, { immediate: true })

async function reload() {
  if (!store.isOpen) return
  const env = whichEnvironment.value ?? store.environments[0]?.id ?? null
  const res = await commands.resourceTables(env)
  if (res.status === 'ok') {
    tables.value = res.data
    if (!tabs.value.some((t) => t.title === activeTab.value)) {
      activeTab.value = tabs.value[0]?.title ?? CONNECTIONS
    }
    store.resourceTab = activeTab.value
  } else {
    store.error = (res.error as { message?: string })?.message ?? String(res.error)
  }
}

/* ── 環境選單 ──────────────────────────────────────────────
 *
 * 選環境與「管理環境…」放在同一顆鈕底下，因為它們是同一個問題的兩半：
 * 「我要看哪個環境」跟「到底有哪些環境」。分成兩個入口的話，第二個
 * 會沒有地方放——它不屬於任何一張表。
 */

const envMenuOpen = ref(false)
const managingEnvironments = ref(false)

/** 邏輯層是母版，跟環境無關，所以那時候不列環境。 */
const environmentMatters = computed(
  () => Boolean(currentTable.value?.environment) || onConnections.value,
)

const currentEnvironment = computed(
  () => whichEnvironment.value ?? store.environments[0]?.id ?? null,
)

function closeEnvMenu() {
  envMenuOpen.value = false
}

function toggleEnvMenu() {
  envMenuOpen.value = !envMenuOpen.value
  // 點畫面別處就收起來。鍵盤的 Escape 由樣板上的 @keydown 處理。
  if (envMenuOpen.value) window.addEventListener('click', closeEnvMenu, { once: true })
}

onUnmounted(() => window.removeEventListener('click', closeEnvMenu))

function pickEnvironment(id: string) {
  whichEnvironment.value = id
  envMenuOpen.value = false
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

/**
 * 新增一列。
 *
 * 三個動作都收 `table`，預設是現在那一頁——「管理環境…」那個對話框
 * 用的是同一組，只是傳的是環境那張表。兩邊各寫一份的話，
 * 遲早只有一邊記得「空白資源的 id 要跟 Rust 要」。
 */
async function addNew(table: Table | null = currentTable.value) {
  if (!table) {
    if (!onConnections.value) return
    contract.value = store.relationships[0]?.id ?? ''
    pickingContract.value = true
    return
  }
  const res = await commands.blankResource(table.kind, table.environment, null)
  if (res.status !== 'ok') {
    store.error = (res.error as { message?: string })?.message ?? String(res.error)
    return
  }
  store.editingResource = { resource: res.data, isNew: true, kind: table.title }
}

function edit(row: ResourceRow, table: Table | null = currentTable.value) {
  // 表格每一列都帶著自己的 `Resource`，表單直接拿它當起點。
  // 讓前端從 project 裡自己撈的話，要知道每種資源住在哪一層——那是模型知識。
  store.editingResource = { resource: row.resource, isNew: false, kind: table?.title ?? '' }
}

function askDelete(row: ResourceRow, table: Table | null = currentTable.value) {
  store.deleting = {
    edit: { deleteResource: row.resource },
    kind: table?.title ?? '東西',
    label: row.cells[0] ?? row.id,
  }
}
</script>

<template>
  <div class="wrap">
    <!-- 第三層：資源種類。導覽三層裡最安靜的一層。
         母版與分身之間有一條線——那個分界是整個領域模型最重要的一件事，
         畫成同一串等於在說「這些都差不多」。 -->
    <div class="tabs">
      <template v-for="(t, i) in tabs" :key="t.title">
        <span
          v-if="t.group !== tabs[i - 1]?.group"
          class="group"
          :class="{ second: i > 0 }"
        >{{ GROUP_LABEL[t.group] ?? '' }}</span>
        <button
          :class="{ on: activeTab === t.title }"
          @click="activeTab = t.title"
        >
          {{ t.title }}
          <span class="n">{{ t.count }}</span>
        </button>
      </template>

      <span class="grow" />

      <!-- 選環境與「管理環境…」在同一顆鈕底下：它們是同一個問題的兩半。 -->
      <span v-if="store.environments.length" class="env" @keydown.esc="envMenuOpen = false">
        <button
          class="pick"
          :aria-expanded="envMenuOpen"
          @click.stop="toggleEnvMenu()"
        >
          環境<template v-if="environmentMatters">：{{ store.envName(currentEnvironment!) }}</template>
          <span class="caret">▾</span>
        </button>
        <div v-if="envMenuOpen" class="menu" @click.stop>
          <!-- 邏輯層是母版，跟環境無關，那時候列環境只會讓人以為
               這一頁的內容會跟著變。 -->
          <template v-if="environmentMatters && store.environments.length > 1">
            <button
              v-for="e in store.environments" :key="e.id"
              @click="pickEnvironment(e.id)"
            >
              <span class="tick">{{ currentEnvironment === e.id ? '✓' : '' }}</span>
              {{ e.slug }}
            </button>
            <span class="sep" />
          </template>
          <button class="manage" @click="envMenuOpen = false; managingEnvironments = true">
            <span class="tick" />
            管理環境…
          </button>
        </div>
      </span>

      <button class="primary add" :disabled="store.busy" @click="addNew()">
        ＋ 新增{{ onConnections ? CONNECTIONS : currentTable?.title }}
      </button>
    </div>

    <!-- 「環境」那張表不在分頁列上，因為它列的是全部環境，不隸屬於任何一個。
         使用者要找它的時候會去按環境選單，所以它就開在那裡。 -->
    <div v-if="managingEnvironments" class="scrim" @click.self="managingEnvironments = false">
      <section class="box wide" role="dialog" aria-modal="true" aria-label="管理環境">
        <h2>管理環境</h2>
        <p class="muted lead">
          環境是分身的容器。這裡改的是<strong>有哪些環境</strong>，
          不是某一個環境裡面有什麼。
        </p>
        <div class="env-rows">
          <p v-if="!environmentTable?.rows.length" class="empty muted">
            {{ environmentTable?.emptyHint }}
          </p>
          <table v-else>
            <thead>
              <tr>
                <th class="sev" />
                <th v-for="c in environmentTable.columns" :key="c">{{ c }}</th>
                <th class="act" />
              </tr>
            </thead>
            <tbody>
              <tr v-for="row in environmentTable.rows" :key="row.id">
                <td class="sev">
                  <span v-if="row.severity" :class="['dot', row.severity]" />
                </td>
                <td
                  v-for="(cell, i) in row.cells" :key="i"
                  :class="{ mono: i === 0, muted: i > 0 }"
                >{{ cell }}</td>
                <td class="act">
                  <button class="icon" :disabled="store.busy" title="編輯" @click="edit(row, environmentTable)">✎</button>
                  <button class="icon del" :disabled="store.busy" title="刪除" @click="askDelete(row, environmentTable)">✕</button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <footer>
          <button
            class="primary"
            :disabled="store.busy || !environmentTable"
            @click="addNew(environmentTable)"
          >＋ 新增環境</button>
          <span class="grow" />
          <button @click="managingEnvironments = false">關閉</button>
        </footer>
      </section>
    </div>

    <!-- 新增連線要先問補給哪一條契約。連線一定屬於某條契約——
         沒有契約的連線 lint 會叫（L003），所以這裡不給「不選」這個選項。 -->
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

      <table v-else :class="{ fixed: head?.frozen }">
        <TableHead
          ref="head"
          :columns="currentTable.columns"
          :visible="columns"
          :prefs="prefs"
          :sort="sort"
          :table-key="currentTable.kind"
          @update:sort="sort = $event"
          @toggle="toggle"
          @resize="setWidth"
          @autofit="clearWidth"
        >
          <template #lead><th class="sev" /></template>
        </TableHead>
        <tbody>
          <tr v-for="row in visibleRows" :key="row.id">
            <td class="sev">
              <span v-if="row.severity" :class="['dot', row.severity]" :title="'這一列有 lint 問題'" />
            </td>
            <td
              v-for="(c, i) in columns" :key="c"
              :class="{ mono: i === 0, muted: i > 0 }"
              :style="i === 0 && row.depth ? { paddingLeft: `${12 + row.depth * 18}px` } : undefined"
            >{{ cellOf(row, c) }}</td>
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
.n { margin-left: 5px; font-size: 11px; color: var(--ink-4); font-variant-numeric: tabular-nums; }
.grow { flex: 1; }
.add { font-size: 12.5px; padding: 3px 10px; }

/* 母版與分身的分界。第二組前面多一條線與一點呼吸空間。 */
.group {
  font-family: var(--mono);
  font-size: 10.5px;
  font-weight: 600;
  letter-spacing: .1em;
  color: var(--ink-4);
  padding-right: 6px;
  white-space: nowrap;
}
.group.second { margin-left: 10px; padding-left: 12px; border-left: 1px solid var(--rule); }

/* ── 環境選單 ────────────────────────────────────────── */

.env { position: relative; display: inline-flex; }
.pick { font-size: 12px; padding: 3px 9px; }
.caret { margin-left: 5px; color: var(--ink-4); }

.menu {
  position: absolute;
  top: calc(100% + 5px);
  right: 0;
  z-index: 15;
  min-width: 168px;
  padding: 5px 0;
  border: 1px solid var(--rule);
  border-radius: 7px;
  background: var(--raise);
  box-shadow: 0 8px 24px var(--shadow);
}
.menu button {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 5px 12px;
  border: 0;
  border-radius: 0;
  background: transparent;
  font-size: 12.5px;
  text-align: left;
}
.menu button:hover { background: var(--surface-2); }
.menu .tick { width: 12px; flex: none; color: var(--warp); font-weight: 700; }
.menu .sep { display: block; height: 1px; margin: 5px 0; background: var(--rule); }
.menu .manage { font-weight: 600; }

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
/* 管理環境那個框裡有一張表，比「挑一條契約」那個框寬。 */
.box.wide { width: min(560px, 94%); }
.env-rows { max-height: 300px; overflow: auto; border: 1px solid var(--rule-2); border-radius: 6px; }
.env-rows .empty { padding: 24px 16px; text-align: center; line-height: 1.7; }
.env-rows table { width: 100%; }
.env-rows thead th { background: var(--surface-2); }

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
/* 量完欄寬之後才切成 fixed——理由見 `TableHead.vue`。 */
/* `width` 維持 `max-content`，不能改成 `100%`。
   欄位很多的時候（每個環境一欄）這張表本來就比視窗寬，要橫向捲動；
   改成 100% 會把它壓回視窗寬度，然後 fixed 佈局就開始把每一欄
   截成刪節號——看起來像資料不見了。 */
table.fixed { table-layout: fixed; }
table.fixed td { overflow: hidden; text-overflow: ellipsis; }

/* 欄名列由 `TableHead` 畫，它有自己的樣式。這裡只管內容與插進去的那幾格。 */
th, td {
  text-align: left;
  padding: 0 12px;
  height: var(--row);
  border-bottom: 1px solid var(--rule-2);
  white-space: nowrap;
}
tbody tr:hover td { background: var(--surface-2); }

.sev { width: 26px; padding-right: 0; }
thead .sev { background: var(--surface-2); border-bottom: 1px solid var(--rule); }
.dot { display: inline-block; width: 7px; height: 7px; border-radius: 50%; }
.dot.error { background: var(--broken); }
.dot.warning { background: var(--warn); }

/* 欄名列的最後一格放「欄位」那顆鈕，所以這一欄要跟它一樣寬。 */
.act { width: 66px; padding-left: 0; padding-right: 8px; text-align: right; }
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
