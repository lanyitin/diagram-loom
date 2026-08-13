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
import { useProject } from '../lib/store'
import type { Cell, Id } from '../lib/model'

const store = useProject()

/** 格子裡那行字。數字由 Rust 算好，這裡只負責排版。 */
function 摘要(cell: Cell | undefined): string {
  if (!cell || cell.status === 'missing') return '未實現'
  const 段 = `${cell.segments} 段`
  if (cell.expect !== null && cell.expect !== cell.targets) {
    return `${段} · ${cell.targets}／${cell.expect} 台`
  }
  return cell.targets > 1 ? `${段} · ${cell.targets} 台` : 段
}

/** 滑鼠停留時的說明。把規則代號攤開，使用者不必記 L004 是什麼。 */
function 說明(cell: Cell | undefined, rel: Id, env: Id): string {
  const 位置 = `${rel} / ${store.環境名(env)}`
  if (!cell) return 位置
  if (cell.rules.length === 0) return `${位置}：沒有問題`
  const 相關 = store.發現.filter(
    (f) => f.environment === env && cell.rules.includes(f.rule),
  )
  return [位置, ...相關.map((f) => `${f.rule} ${f.detail}`)].join('\n')
}
</script>

<template>
  <div class="matrix">
    <table>
      <thead>
        <tr>
          <th class="rel">邏輯連線（契約）</th>
          <th class="purpose">用途</th>
          <th v-for="env in store.顯示的環境" :key="env.id" class="env">
            {{ env.slug }}
          </th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="rel in store.顯示的契約" :key="rel.id">
          <td class="rel mono">{{ rel.slug }}</td>
          <td class="purpose muted">{{ rel.purpose }}</td>
          <td
            v-for="env in store.顯示的環境"
            :key="env.id"
            class="cell"
            :title="說明(store.格子(rel.id, env.id), rel.slug, env.id)"
          >
            <span :class="['chip', store.格子(rel.id, env.id)?.status ?? 'missing']">
              {{ 摘要(store.格子(rel.id, env.id)) }}
            </span>
          </td>
        </tr>
        <tr v-if="store.顯示的契約.length === 0">
          <td :colspan="store.顯示的環境.length + 2" class="empty muted">
            {{ store.契約.length === 0 ? '這個專案還沒有定義任何邏輯連線。' : '沒有符合條件的列。' }}
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
  z-index: 2;
  background: var(--surface-2);
  border-bottom: 1px solid var(--rule);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: .06em;
  text-transform: uppercase;
  color: var(--ink-3);
}

/* 契約那欄橫向捲動時要釘住，否則捲到第 8 個環境就不知道自己在看哪一列。 */
.rel {
  position: sticky;
  left: 0;
  z-index: 1;
  background: var(--surface);
  border-right: 1px solid var(--rule);
  min-width: 240px;
}
thead .rel { z-index: 3; background: var(--surface-2); }
tbody tr:hover .rel { background: var(--surface-2); }

.purpose { max-width: 260px; overflow: hidden; text-overflow: ellipsis; }
.env { min-width: 132px; }
.cell { min-width: 132px; }
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
  color: #fff;
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
