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
 */
import { computed, ref } from 'vue'
import { useProject } from '../lib/store'
import { nextSort, sortBy, type Sort } from '../lib/rows'
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

/**
 * 可以排序的欄位。
 *
 * **用欄名認，不用第幾欄**：「實際／期望」那欄會依內容出現或消失，
 * 用位置的話會排到別欄去，而且看起來像排錯了而不是抓錯欄。
 */
const KEYS: Record<string, (r: Row) => string | number> = {
  契約: (r) => r.servesSlug ?? r.serves,
  來源: (r) => side(r.from),
  目標: (r) => side(r.to),
  目標位址: (r) => addressOf(r.to),
  // 差幾台。遞增就是「缺最多的排前面」——點這一欄的人要找的就是對不上的。
  '實際／期望': (r) => (r.to.expect === null ? 0 : r.to.matched - r.to.expect),
  用途: (r) => r.purpose,
}

const sort = ref<Sort<string> | null>(null)

const rows = computed(() =>
  sortBy(store.visibleRows, sort.value, (r, column) => KEYS[column]!(r)),
)

function clickHeader(column: string) {
  sort.value = nextSort(sort.value, column)
}

/** 標題上的三個屬性都一樣，集中在這裡，不要在樣板上抄六遍。 */
function head(column: string) {
  const state = sort.value?.column !== column
    ? 'none'
    : sort.value.direction === 'asc' ? 'ascending' : 'descending'
  return {
    class: 'sortable',
    'aria-sort': state as 'none' | 'ascending' | 'descending',
    onClick: () => clickHeader(column),
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
</script>

<template>
  <div class="wrap">
    <table>
      <thead>
        <tr>
          <th class="sev" />
          <th class="kind" />
          <th v-bind="head('契約')">契約</th>
          <th v-bind="head('來源')">來源</th>
          <th v-bind="head('目標')">目標</th>
          <th v-bind="head('目標位址')">目標位址</th>
          <th v-if="anyRowHasExpect" v-bind="head('實際／期望')" class="right sortable">實際／期望</th>
          <th v-bind="head('用途')">用途</th>
          <th class="act" />
        </tr>
      </thead>
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
          <td class="mono">{{ row.servesSlug ?? row.serves }}</td>
          <td class="mono">{{ side(row.from) }}</td>
          <td class="mono">{{ side(row.to) }}</td>
          <td class="mono muted">{{ addressOf(row.to) }}</td>
          <td v-if="anyRowHasExpect" class="right mono num">
            <template v-if="row.to.expect !== null">
              <span :class="{ mismatch: row.to.matched !== row.to.expect }">
                {{ row.to.matched }}／{{ row.to.expect }}
              </span>
            </template>
          </td>
          <td class="muted">{{ row.purpose }}</td>
          <!-- 刪除只在滑到那一列時才出現。它是這張表上唯一會弄丟資料的動作，
               不該跟其他欄位一樣一直亮著等人誤觸。 -->
          <td class="act">
            <button class="del" :disabled="store.busy" title="刪除這條連線" @click="askDelete(row)">
              ✕
            </button>
          </td>
        </tr>
        <tr v-if="rows.length === 0">
          <td :colspan="9" class="empty muted">沒有符合條件的連線。</td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.wrap { flex: 1; min-height: 0; overflow: auto; background: var(--surface); }

table { border-collapse: separate; border-spacing: 0; width: max-content; min-width: 100%; }

th, td {
  text-align: left;
  padding: 0 12px;
  height: var(--row);
  border-bottom: 1px solid var(--rule-2);
  white-space: nowrap;
}

th.sortable { cursor: pointer; user-select: none; }
th.sortable:hover { color: var(--ink); }
/*
 * 箭頭用 CSS 畫，不放進標題的文字裡——放進去的話欄名會變成「契約 ▴」，
 * 螢幕閱讀器會照唸，而排序狀態 `aria-sort` 已經講過一次了。
 * 透明的那個一直佔著位置，不然排序時整排標題會左右跳。
 */
th.sortable::after { content: '▴'; margin-left: 4px; font-size: 10px; opacity: 0; }
th.sortable[aria-sort='ascending']::after { opacity: 0.75; }
th.sortable[aria-sort='descending']::after { content: '▾'; opacity: 0.75; }

thead th {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--surface-2);
  border-bottom: 1px solid var(--rule);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: .06em;
  text-transform: uppercase;
  color: var(--ink-3);
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
.act { width: 34px; padding-left: 0; padding-right: 8px; text-align: right; }
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

/* 數量對不上時把數字本身標起來——這是萬用字元最容易漏的地方。 */
.mismatch { color: var(--broken); font-weight: 600; }
</style>
