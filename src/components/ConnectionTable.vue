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
import { computed } from 'vue'
import { useProject } from '../lib/store'
import type { Side } from '../lib/model'

const store = useProject()

/** 位址可能有十幾個，表格裡只放第一個加一個數量。 */
function 位址(side: Side): string {
  if (side.addresses.length === 0) return ''
  if (side.addresses.length === 1) return side.addresses[0]!
  return `${side.addresses[0]} +${side.addresses.length - 1}`
}

/** 端點的顯示：名字加接點。 */
function 端(side: Side): string {
  return side.endpoint ? `${side.label} : ${side.endpoint}` : side.label
}

const 有數量的列 = computed(() => store.顯示的列.some((r) => r.to.expect !== null))
</script>

<template>
  <div class="wrap">
    <table>
      <thead>
        <tr>
          <th class="sev" />
          <th class="kind" />
          <th>契約</th>
          <th>來源</th>
          <th>目標</th>
          <th>目標位址</th>
          <th v-if="有數量的列" class="right">實際／期望</th>
          <th>用途</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in store.顯示的列" :key="row.id" :class="{ fallback: row.kind === 'fallback' }" :title="row.rules.join('、')">
          <td class="sev">
            <span v-if="row.severity" :class="['dot', row.severity]" />
          </td>
          <!-- 備援路徑要看得出不一樣，否則四條線一樣重，
               讀的人分不出平常的資料流是哪幾條。 -->
          <td class="kind">
            <span v-if="row.kind === 'fallback'" class="fb" title="備援路徑：只在故障時走">備援</span>
          </td>
          <td class="mono">{{ row.servesSlug ?? row.serves }}</td>
          <td class="mono">{{ 端(row.from) }}</td>
          <td class="mono">{{ 端(row.to) }}</td>
          <td class="mono muted">{{ 位址(row.to) }}</td>
          <td v-if="有數量的列" class="right mono num">
            <template v-if="row.to.expect !== null">
              <span :class="{ 對不上: row.to.matched !== row.to.expect }">
                {{ row.to.matched }}／{{ row.to.expect }}
              </span>
            </template>
          </td>
          <td class="muted">{{ row.purpose }}</td>
        </tr>
        <tr v-if="store.顯示的列.length === 0">
          <td :colspan="8" class="empty muted">沒有符合條件的連線。</td>
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
.dot { display: inline-block; width: 7px; height: 7px; border-radius: 50%; }
.dot.error { background: var(--broken); }
.dot.warning { background: var(--warn); }

/* 數量對不上時把數字本身標起來——這是萬用字元最容易漏的地方。 */
.對不上 { color: var(--broken); font-weight: 600; }
</style>
