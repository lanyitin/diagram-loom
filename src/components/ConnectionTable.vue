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
 */
import { computed, ref } from 'vue'
import { useProject } from '../lib/store'
import { useColumns } from '../lib/columns'
import { sortBy, type Sort } from '../lib/rows'
import TableHead from './TableHead.vue'
import type { Row, Side } from '../lib/model'

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
  ['契約', '來源', '目標', '目標位址', EXPECT, '用途']
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
    case '用途': return row.purpose
    default: return ''
  }
}

/** 一格的樣式。識別碼用等寬字，附註用灰的，數字靠右。 */
function cellClass(row: Row, column: string) {
  return {
    mono: column !== '用途',
    muted: column === '目標位址' || column === '用途',
    right: column === EXPECT,
    num: column === EXPECT,
    // 數量對不上時把數字本身標起來——這是萬用字元最容易漏的地方。
    mismatch: column === EXPECT && row.to.expect !== null && row.to.matched !== row.to.expect,
  }
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
               讀的人分不出平常的資料流是哪幾條。 -->
          <td class="kind">
            <span v-if="row.kind === 'fallback'" class="fb" title="備援路徑：只在故障時走">備援</span>
          </td>
          <td v-for="c in columns" :key="c" :class="cellClass(row, c)">{{ text(row, c) }}</td>
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
.fb {
  display: inline-block;
  padding: 0 5px;
  border: 1px dashed color-mix(in srgb, var(--ink-3) 70%, transparent);
  border-radius: 3px;
  font-size: 11px;
  color: var(--ink-3);
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
