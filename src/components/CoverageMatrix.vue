<script setup lang="ts">
/**
 * 覆蓋矩陣：每條邏輯連線 × 每個環境。
 *
 * # 這個畫面在回答什麼
 *
 * 不是「有哪些連線」，是**「我漏了什麼」**。一張攤平的連線表只顯示已經有的
 * 東西，而漏掉的東西本來就不在表上。矩陣把「每個環境都要實現每條契約」
 * 這條通則畫成方格，**空格就是漏洞**。
 *
 * # 為什麼格子裡不是打勾
 *
 * 打勾只回答「有沒有」。真正會咬人的是「prod 走 F5 兩段、dev 直連一段」
 * 跟「12 台還是 14 台」這種差異，所以格子裡放「幾段 · 幾台」。
 *
 * # 狀態不能只靠顏色
 *
 * 缺漏的格子沒有填色、用虛線框，看起來就是**空的**；壞掉的格子有實心色條。
 * 印成黑白、或色覺不同的人，都還是分得出來。
 */
import { computed, ref } from 'vue'
import { useProject } from '../lib/store'
import { useColumns } from '../lib/columns'
import { sortBy, type Sort } from '../lib/rows'
import TableHead from './TableHead.vue'
import type { Cell, Id, Relationship } from '../lib/model'

const store = useProject()

const CONTRACT = '邏輯連線（契約）'
const PURPOSE = '用途'

/**
 * 欄就是「契約、用途，加上每個環境一欄」。
 *
 * 環境那幾欄由工具列上的環境勾選決定，所以這張表**不放「欄位」選單**——
 * 同一件事有兩個入口的話，使用者兩個都不會信。剩下的拖寬與排序照舊。
 */
const columns = computed(() => [
  CONTRACT,
  PURPOSE,
  ...store.visibleEnvironments.map((e) => e.slug),
])

/**
 * 環境那幾欄照「有多糟」排。
 *
 * 遞增就是**最糟的排前面**——點某個環境那一欄的人要找的就是那個環境
 * 還缺什麼，不是照字母看熱鬧。跟連線表的「實際／期望」同一個約定。
 */
const RANK: Record<string, number> = { missing: 0, broken: 1, warning: 2, realized: 3 }

const sort = ref<Sort | null>(null)

function key(rel: Relationship, column: string): string | number {
  if (column === CONTRACT) return rel.slug
  if (column === PURPOSE) return rel.purpose
  const env = store.visibleEnvironments.find((e) => e.slug === column)
  if (!env) return 0
  return RANK[store.cell(rel.id, env.id)?.status ?? 'missing'] ?? 0
}

const relationships = computed(() => sortBy(store.visibleRelationships, sort.value, key))

const { prefs, visible, setWidth, clearWidth } = useColumns(ref('matrix'))
const head = ref<InstanceType<typeof TableHead> | null>(null)

/** 格子裡那行字。數字由 Rust 算好，這裡只負責排版。 */
function summary(cell: Cell | undefined): string {
  if (!cell || cell.status === 'missing') return '未實現'
  const segments = `${cell.segments} 段`
  if (cell.expect !== null && cell.expect !== cell.targets) {
    return `${segments} · ${cell.targets}／${cell.expect} 台`
  }
  return cell.targets > 1 ? `${segments} · ${cell.targets} 台` : segments
}

/** 滑鼠停留時的說明。把規則代號攤開，使用者不必記 L004 是什麼。 */
function description(cell: Cell | undefined, rel: Id, env: Id): string {
  const position = `${rel} / ${store.envName(env)}`
  if (!cell) return position
  if (cell.rules.length === 0) return `${position}：沒有問題`
  const related = store.findings.filter(
    (f) => f.environment === env && cell.rules.includes(f.rule),
  )
  return [position, ...related.map((f) => `${f.rule} ${f.detail}`)].join('\n')
}
</script>

<template>
  <div class="matrix">
    <table :class="{ fixed: head?.frozen }">
      <TableHead
        ref="head"
        :columns="columns"
        :visible="visible(columns)"
        :prefs="prefs"
        :sort="sort"
        :table-key="`matrix-${store.visibleEnvironments.length}`"
        :menu="false"
        @update:sort="sort = $event"
        @resize="setWidth"
        @autofit="clearWidth"
      />
      <tbody>
        <tr v-for="rel in relationships" :key="rel.id">
          <td class="rel mono">{{ rel.slug }}</td>
          <td class="purpose muted">{{ rel.purpose }}</td>
          <td
            v-for="env in store.visibleEnvironments"
            :key="env.id"
            class="cell"
            :title="description(store.cell(rel.id, env.id), rel.slug, env.id)"
          >
            <span :class="['chip', store.cell(rel.id, env.id)?.status ?? 'missing']">
              {{ summary(store.cell(rel.id, env.id)) }}
            </span>
          </td>
        </tr>
        <tr v-if="store.visibleRelationships.length === 0">
          <td :colspan="store.visibleEnvironments.length + 2" class="empty muted">
            {{ store.relationships.length === 0 ? '這個專案還沒有定義任何邏輯連線。' : '沒有符合條件的列。' }}
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.matrix {
  flex: 1;
  min-height: 0;
  overflow: auto;          /* 環境勾多了就橫向捲動 */
  background: var(--surface);
}

table {
  border-collapse: separate;
  border-spacing: 0;
  width: max-content;
  min-width: 100%;
}

/* 量完欄寬之後才切成 fixed——理由見 `TableHead.vue`。
   `width` 維持 `max-content`：環境勾多了這張表本來就比視窗寬。 */
table.fixed { table-layout: fixed; }
table.fixed td { overflow: hidden; text-overflow: ellipsis; }

td {
  text-align: left;
  padding: 0 12px;
  height: var(--row);
  border-bottom: 1px solid var(--rule-2);
  white-space: nowrap;
}

/*
 * 契約那欄橫向捲動時要釘住，否則捲到第 8 個環境就不知道自己在看哪一列。
 *
 * 欄名列由 `TableHead` 畫，它只管「往上釘」（`top: 0`）——「往左釘」
 * 是這張表獨有的需求，所以用 `:deep()` 從外面補第一格。
 */
.rel {
  position: sticky;
  left: 0;
  z-index: 1;
  background: var(--surface);
  border-right: 1px solid var(--rule);
  min-width: 240px;
}
tbody tr:hover .rel { background: var(--surface-2); }

.matrix :deep(thead th:first-child) {
  position: sticky;
  left: 0;
  z-index: 3;
  border-right: 1px solid var(--rule);
  min-width: 240px;
}

.purpose { max-width: 260px; overflow: hidden; text-overflow: ellipsis; }
.cell { min-width: 132px; }
/* 環境那幾欄的欄名也要有下限，不然只有 dev 這種短名字時欄會縮到
   比格子裡的「2 段 · 3 台」還窄，量出來的寬度就是錯的。 */
.matrix :deep(thead th:nth-child(n + 3)) { min-width: 132px; }
tbody tr:hover td { background: var(--surface-2); }

.empty { text-align: center; height: 96px; }

/* ── 格子 ──────────────────────────────────────────── */

.chip {
  display: inline-block;
  padding: 1px 9px;
  border-radius: 4px;
  border: 1px solid transparent;
  font-family: var(--mono);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}

.chip.realized {
  color: var(--ok);
  border-color: color-mix(in srgb, var(--ok) 35%, transparent);
  background: color-mix(in srgb, var(--ok) 8%, transparent);
}

.chip.warning {
  color: var(--warn);
  border-color: color-mix(in srgb, var(--warn) 40%, transparent);
  background: color-mix(in srgb, var(--warn) 10%, transparent);
}

/* 壞掉：實心的色條，看起來像警報。 */
.chip.broken {
  color: var(--on-accent);
  background: var(--broken);
  border-color: var(--broken);
  font-weight: 600;
}

/* 缺漏：不填色、虛線框——讓它看起來就是「空的」。
   這格是使用者最怕的東西，用「空」本身當訊號比塗紅色更貼切。 */
.chip.missing {
  color: var(--missing);
  border: 1px dashed color-mix(in srgb, var(--missing) 55%, transparent);
  background: none;
  font-weight: 600;
}
</style>
