<script setup lang="ts">
/**
 * 連線表：一個環境的所有實際連線，攤平。
 *
 * 矩陣負責**發現**漏洞，這張表負責**逐條核對**。
 *
 * # 為什麼不依契約分組
 *
 * 分組讀起來是清楚一點，但排序與篩選會變複雜（要決定組內排序、組要不要
 * 一起排、篩掉組內全部之後組還在不在）。先用篩選解決，真的不夠用再說。
 *
 * # 名稱與位址都是 Rust 解析好的
 *
 * `redis-*` 展開成幾台、位址從哪個 Endpoint 來——那跟 lint 做萬用字元
 * 比對是同一件事。這裡只負責排版。
 *
 * # 欄名列共用 [`TableHead`]
 *
 * 排序、拖寬、挑欄位三件事跟資源表一模一樣，所以是同一個元件。
 * 各寫一份的話兩張表遲早會長出兩種操作感，而使用者只會覺得這個工具
 * 前後不一致。
 *
 * # 這張表上改得動什麼
 *
 * | | 在哪改 |
 * | --- | --- |
 * | 用途、備註 | **就地**：點一格就變輸入框 |
 * | 正常／備援 | **就地**：最左邊那一格點一下就切 |
 * | 期望數量 | 仍然只在 Lint 面板 |
 * | 兩端（來源／目標） | **改不了**，只能刪掉重建 |
 *
 * 期望數量留在 Lint 面板，是因為它要的是一個數字，而且只有規則正在叫的
 * 時候那個數字才有意義——平常那一格是「現在幾台／該幾台」的比對結果，
 * 不是一個等著被填的空格。
 *
 * 兩端改不了是因為核心根本沒有那支 `Edit`：要換的是整個 `Endpointing`
 * （解析過的兩端），不是一格字。而且連線沒有名字，它的身分**就是**
 * 「服務哪條契約 + 兩端接到哪」——改了兩端在模型上已經是另一條連線。
 * 見 `docs/editing.md` 的「還沒做」。
 */
import { computed, nextTick, ref } from 'vue'
import { useProject } from '../lib/store'
import { useColumns } from '../lib/columns'
import { sortBy, type Sort } from '../lib/rows'
import TableHead from './TableHead.vue'
import type { Edit, Id, Row, Side } from '../lib/model'

const store = useProject()

/** 位址可能有十幾個，表格裡只放第一個加一個數量。 */
function addressOf(side: Side): string {
  if (side.addresses.length === 0) return ''
  if (side.addresses.length === 1) return side.addresses[0]!
  return `${side.addresses[0]} +${side.addresses.length - 1}`
}

/** 端點的顯示：名字加接點。 */
function side(side: Side): string {
  return side.endpoint ? `${side.label} : ${side.endpoint}` : side.label
}

const anyRowHasExpect = computed(() => store.visibleRows.some((r) => r.to.expect !== null))

const EXPECT = '實際／期望'

/**
 * 這張表可能有哪些欄。
 *
 * 「實際／期望」在**沒有任何一列用得到**的時候整欄不出現。那不是使用者
 * 的偏好，是「這裡永遠是空白」——所以它擋在偏好前面，而不是變成
 * 選單裡一個勾了也看不到東西的選項。
 */
const available = computed(() =>
  ['契約', '來源', '目標', '目標位址', EXPECT, '用途', '備註']
    .filter((c) => c !== EXPECT || anyRowHasExpect.value),
)

const { prefs, visible, toggle, setWidth, clearWidth } = useColumns(ref('connections'))
const columns = computed(() => visible(available.value))

/**
 * 每一欄怎麼排序。
 *
 * **用欄名認，不用第幾欄**：欄位會依內容與使用者的選擇增減，
 * 用位置的話會排到別欄去，而且看起來像排錯了而不是抓錯欄。
 */
const KEYS: Record<string, (r: Row) => string | number> = {
  契約: (r) => r.servesSlug ?? r.serves,
  來源: (r) => side(r.from),
  目標: (r) => side(r.to),
  目標位址: (r) => addressOf(r.to),
  // 差幾台。遞增就是「缺最多的排前面」——點這一欄的人要找的就是對不上的。
  [EXPECT]: (r) => (r.to.expect === null ? 0 : r.to.matched - r.to.expect),
  用途: (r) => r.purpose,
  備註: (r) => r.memo,
}

const sort = ref<Sort | null>(null)

const rows = computed(() =>
  sortBy(store.visibleRows, sort.value, (r, column) => KEYS[column]!(r)),
)

/** 一格裡要印的字。 */
function text(row: Row, column: string): string {
  switch (column) {
    case '契約': return row.servesSlug ?? row.serves
    case '來源': return side(row.from)
    case '目標': return side(row.to)
    case '目標位址': return addressOf(row.to)
    case EXPECT: return row.to.expect === null ? '' : `${row.to.matched}／${row.to.expect}`
    // 可編欄位的讀法只有一處：`EDITABLE`。抄第二份的話，顯示的跟編輯前
    // 帶進去的會有一天不一樣，而那看起來像「打開就自己改了」。
    default: return EDITABLE[column]?.read(row) ?? ''
  }
}

/** 一格的樣式。識別碼用等寬字，附註用灰的，數字靠右。 */
function cellClass(row: Row, column: string) {
  return {
    mono: column !== '用途' && column !== '備註',
    muted: column === '目標位址' || column === '用途' || column === '備註',
    right: column === EXPECT,
    num: column === EXPECT,
    // 備註是自由文字，長度不受控。不折行的話它會把整欄撐到幾百 px 寬，
    // 於是別的欄全被推出視窗外——而使用者是來「一眼掃過幾百列」的。
    wrap: column === '備註',
    // 數量對不上時把數字本身標起來——這是萬用字元最容易漏的地方。
    mismatch: column === EXPECT && row.to.expect !== null && row.to.matched !== row.to.expect,
  }
}

/**
 * 這張表上就地改得動的欄位。
 *
 * # 為什麼不開一個編輯表單
 *
 * 連線不是 `Resource`，所以它走不到那張通用表單，也不該為了幾個欄位長出一個
 * 對話框——這張表本來就是「一列一列逐條核對」用的，改一個字要開一扇窗
 * 會把那件事打斷。
 *
 * # 為什麼是一張表，不是三段各寫一次
 *
 * 每一種「改連線的某個欄位」在 Rust 那邊都是自己一支 `Edit`（連線不是
 * `Resource`，沒有 `UpdateResource` 可走），所以這裡也一欄一支，一對一對得上。
 * 前端不決定哪個欄位寫進哪裡，只決定畫面。
 */
const EDITABLE: Record<
  string,
  {
    read: (row: Row) => string
    placeholder: string
    edit: (row: Row, next: string) => Edit
  }
> = {
  用途: {
    read: (r) => r.purpose,
    placeholder: '這條連線是做什麼用的',
    // `environment` 一定要給。給 null 的話 Rust 會去**邏輯層**找同 id 的契約
    // （`SetPurpose` 有兩個分支），而連線 id 在那裡不存在——整次編輯失敗，
    // 畫面上只看得到「按了沒反應」。
    edit: (r, v) => ({
      setPurpose: { environment: r.environment, subject: r.id, purpose: v },
    }),
  },
  備註: {
    read: (r) => r.memo,
    placeholder: '規則管不到的話寫在這裡',
    edit: (r, v) => ({
      setConnectionMemo: { environment: r.environment, connection: r.id, memo: v },
    }),
  },
}

/**
 * 正在編**哪一格**——不是哪一列。
 *
 * # 為什麼不能只認列
 *
 * 一旦有第二個可編欄位，只認列的話點開「用途」會讓同一列的「備註」**也**
 * 變成輸入框，而且兩個綁同一個 `draft`；blur 哪一個都會照那一格的 `Edit`
 * 送出——結果是**用途的字被寫進備註**。沒有任何錯誤訊息，而備註沒有規則
 * 在看，所以永遠不會被發現。
 */
const editing = ref<{ row: Id; column: string } | null>(null)
const draft = ref('')

function isEditing(row: Row, column: string) {
  return editing.value?.row === row.id && editing.value.column === column
}

/**
 * 正在編的那個輸入框。
 *
 * 用函式 ref 而不是 `ref="editEl"`：那個輸入框長在 `v-for` 底下，
 * Vue 會把同名的 ref 收成一個**陣列**——於是 `.focus()` 會炸在
 * 「不是一個函式」，而畫面看起來只是沒有游標。同一時間只有一格在編，
 * 所以直接指過去就好。
 */
const editEl = ref<HTMLInputElement | null>(null)

async function startEdit(row: Row, column: string) {
  if (store.busy) return
  editing.value = { row: row.id, column }
  draft.value = EDITABLE[column]!.read(row)
  // 展開就把游標放進去。少一次點擊，理由同 `FixEditor`。
  await nextTick()
  editEl.value?.focus()
  editEl.value?.select()
}

async function commit(row: Row, column: string) {
  const field = EDITABLE[column]!
  // 一定要 `trim`：Rust 只有 `SetPurpose` 會削，`SetConnectionMemo` 是原樣存的。
  // 不削的話「等年底汰換　」跟「等年底汰換」是兩份不同的資料，
  // 而畫面上長得一模一樣。
  const next = draft.value.trim()
  editing.value = null
  // 沒改就不要進復原歷史。點開又關掉會多一步 ⌘Z，而那一步什麼都沒做——
  // 使用者按下去會以為自己退掉了真的東西。
  if (next === field.read(row)) return
  await store.applyEdit(field.edit(row, next))
}

/**
 * 正常 ⇄ 備援。
 *
 * # 為什麼是切換，不走上面那套就地編輯
 *
 * `draft` 與 Esc 存在，是因為打字有「打到一半」這個中間狀態，要能整個不算。
 * 切換沒有中間狀態：按下去就是結果，反悔路徑是 ⌘Z——而那件事表單底部
 * 一直在講。
 *
 * # 為什麼不是原生 `<select>`
 *
 * `ResourceForm` 有一條規矩「封閉列舉 → 原生 select」，形式上這也符合。
 * 但那條管的是**表單**：一次填十個欄位，控制項一直亮著是對的。表格是密度
 * 最高的地方，每一列掛一個永遠亮著的 select 會把「哪幾條是備援」這個
 * 一眼可讀的訊號淹掉——而那正是這一欄存在的理由。
 *
 * # 這件事以前只有 Agent 做得到
 *
 * `kind` 過去只有新增連線時勾得到，建完就再也改不了，而 MCP 一直改得動。
 * 人做不到而 Agent 做得到，是最糟的分工。
 */
async function toggleKind(row: Row) {
  if (store.busy) return
  await store.applyEdit({
    setConnectionKind: {
      environment: row.environment,
      connection: row.id,
      kind: row.kind === 'fallback' ? 'primary' : 'fallback',
    },
  })
}

/** 只是打開確認框。真正刪掉在使用者看過影響之後。 */
function askDelete(row: Row) {
  store.deleting = {
    edit: { deleteConnection: { environment: row.environment, connection: row.id } },
    kind: '連線',
    label: `${row.servesSlug ?? row.serves}：${side(row.from)} → ${side(row.to)}`,
  }
}

const head = ref<InstanceType<typeof TableHead> | null>(null)
</script>

<template>
  <div class="wrap">
    <table :class="{ fixed: head?.frozen }">
      <TableHead
        ref="head"
        :columns="available"
        :visible="columns"
        :prefs="prefs"
        :sort="sort"
        table-key="connections"
        :right="[EXPECT]"
        @update:sort="sort = $event"
        @toggle="toggle"
        @resize="setWidth"
        @autofit="clearWidth"
      >
        <template #lead>
          <th class="sev" />
          <th class="kind" />
        </template>
      </TableHead>
      <tbody>
        <tr v-for="row in rows" :key="row.id" :class="{ fallback: row.kind === 'fallback' }" :title="row.rules.join('、')">
          <td class="sev">
            <span v-if="row.severity" :class="['dot', row.severity]" />
          </td>
          <!-- 備援路徑要看得出不一樣，否則四條線一樣重，
               讀的人分不出平常的資料流是哪幾條。

               這一格點得動。「正常」平常留白、滑過去或 focus 才浮出來——
               跟同一列的刪除鈕同一個作法：一整欄都印著「正常」會把這張表
               變得很吵，而大部分的列都是正常。 -->
          <td class="kind">
            <button
              class="kindtoggle"
              :disabled="store.busy"
              :aria-pressed="row.kind === 'fallback'"
              :title="row.kind === 'fallback'
                ? '備援路徑：只在故障時走。點一下改回正常'
                : '正常路徑：平常就在走。點一下改成備援'"
              @click="toggleKind(row)"
            >
              <span v-if="row.kind === 'fallback'" class="fb">備援</span>
              <span v-else class="pri">正常</span>
            </button>
          </td>
          <td v-for="c in columns" :key="c" :class="cellClass(row, c)">
            <template v-if="EDITABLE[c]">
              <input
                v-if="isEditing(row, c)" v-model="draft"
                :ref="(el) => (editEl = el as HTMLInputElement | null)"
                type="text" class="cell-edit" :placeholder="EDITABLE[c]!.placeholder"
                @blur="commit(row, c)"
                @keydown.enter.prevent="commit(row, c)"
                @keydown.esc="editing = null"
              >
              <!-- 空的時候也要點得到，否則沒填過的那幾條永遠填不了第一次——
                   而最需要填的正是空的那幾條（L007 叫的就是空用途）。 -->
              <button v-else class="cell-text" :disabled="store.busy" @click="startEdit(row, c)">
                {{ text(row, c) || '＋' }}
              </button>
            </template>
            <template v-else>{{ text(row, c) }}</template>
          </td>
          <!-- 刪除只在滑到那一列時才出現。它是這張表上唯一會弄丟資料的動作，
               不該跟其他欄位一樣一直亮著等人誤觸。 -->
          <td class="act">
            <button class="del" :disabled="store.busy" title="刪除這條連線" @click="askDelete(row)">
              ✕
            </button>
          </td>
        </tr>
        <tr v-if="rows.length === 0">
          <td :colspan="columns.length + 3" class="empty muted">沒有符合條件的連線。</td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.wrap { flex: 1; min-height: 0; overflow: auto; background: var(--surface); }

table { border-collapse: separate; border-spacing: 0; width: max-content; min-width: 100%; }
/*
 * 量完欄寬之後才切成 fixed。
 *
 * auto 排得比我們好，但那個模式下欄寬拖不動——`nowrap` 的內容就是一道
 * 下限，拉窄了會自己彈回去。切成 fixed 之後才聽我們的，代價是每一欄
 * 都得有明確寬度，所以要先量（見 `TableHead.vue`）。
 */
/* `width` 維持 `max-content`，不能改成 `100%`。
   欄位很多的時候（每個環境一欄）這張表本來就比視窗寬，要橫向捲動；
   改成 100% 會把它壓回視窗寬度，然後 fixed 佈局就開始把每一欄
   截成刪節號——看起來像資料不見了。 */
table.fixed { table-layout: fixed; }
table.fixed td { overflow: hidden; text-overflow: ellipsis; }

td {
  text-align: left;
  padding: 0 12px;
  height: var(--row);
  border-bottom: 1px solid var(--rule-2);
  white-space: nowrap;
}

tbody tr:hover td { background: var(--surface-2); }
tbody tr.fallback td:not(.sev):not(.kind) { opacity: .62; }
.right { text-align: right; }
.empty { text-align: center; height: 96px; }

.sev { width: 26px; padding-right: 0; }

.kind { width: 44px; padding-left: 4px; padding-right: 4px; }
.kindtoggle {
  padding: 0;
  border: none;
  background: transparent;
  font: inherit;
  cursor: pointer;
}
/* 「正常」平常不出現：那是大多數的列，一整欄印滿只會把備援那幾條淹掉。
   `:focus-visible` 那條不能省——用鍵盤的人看不到 hover，
   沒有它的話這顆鈕對他們是隱形的。 */
.pri { opacity: 0; font-size: 11px; color: var(--ink-3); }
tbody tr:hover .kindtoggle .pri,
.kindtoggle:focus-visible .pri { opacity: 1; }
.fb {
  display: inline-block;
  padding: 0 5px;
  border: 1px dashed color-mix(in srgb, var(--ink-3) 70%, transparent);
  border-radius: 3px;
  font-size: 11px;
  color: var(--ink-3);
}
/*
 * 備註折行，而且**要有上限**。
 *
 * 它是自由文字，長度不受控。維持 `nowrap` 的話欄寬會被最長的那一則撐開
 * （欄寬是照 auto 佈局量一次再凍結的，見 `TableHead`），於是後面的欄
 * 全被推出視窗——而這張表的用途正是「一眼掃過幾百列」。
 *
 * `height: var(--row)` 在表格裡是**最小高度**，所以折行的那幾列會自己長高，
 * 不必為它另外調。
 */
td.wrap { white-space: normal; max-width: 36ch; line-height: 1.45; padding-top: 5px; padding-bottom: 5px; }
td.wrap .cell-text { white-space: normal; overflow: visible; }

/* 可編的那幾格：平常看起來就是一格字，滑過去才看得出點得動。
   一直畫成輸入框的話，整片框線會把這張表變得很吵，而這張表的用途正是
   「一眼掃過幾百列」。 */
.cell-text {
  width: 100%;
  padding: 0;
  border: none;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
tbody tr:hover .cell-text:not(:disabled) { text-decoration: underline dotted; cursor: text; }
.cell-edit {
  width: 100%;
  padding: 0 2px;
  border: 1px solid var(--accent, var(--ink-3));
  border-radius: 3px;
  background: var(--surface);
  color: var(--ink);
  font: inherit;
}

/* 欄名列的最後一格放「欄位」那顆鈕，所以這一欄比以前寬。 */
.act { width: 66px; padding-left: 0; padding-right: 8px; text-align: right; }
.del {
  padding: 0 6px;
  border-color: transparent;
  background: transparent;
  color: var(--ink-3);
  opacity: 0;
}
tbody tr:hover .del { opacity: 1; }
.del:hover { color: var(--broken); border-color: color-mix(in srgb, var(--broken) 45%, transparent); }
/* 鍵盤操作看不到 hover，所以 focus 一樣要讓它現身。 */
.del:focus-visible { opacity: 1; }

.dot { display: inline-block; width: 7px; height: 7px; border-radius: 50%; }
.dot.error { background: var(--broken); }
.dot.warning { background: var(--warn); }

.mismatch { color: var(--broken); font-weight: 600; }
</style>
