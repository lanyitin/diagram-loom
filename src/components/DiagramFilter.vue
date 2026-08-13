<script setup lang="ts">
/**
 * 圖的篩選面板。
 *
 * # 這是螢光筆，不是剪刀
 *
 * 勾選只會讓沒被勾到的東西**調暗**，不會拿掉。所以畫面上的字要說
 * 「亮著 / 共」而不是「顯示 / 共」——後者會讓人以為圖被裁過，
 * 進而以為圖上沒有的東西就是資料裡沒有的。那個誤會正是這工具最怕的。
 *
 * # 沒實現的契約也要列
 *
 * 連線數 0 的契約留在清單裡（Rust 那邊也刻意這樣做）。它從清單消失的話
 * 就永遠不會被勾到，而「這個環境少了什麼」正是這工具存在的理由。
 */
import { computed } from 'vue'
import type { Contract } from '../lib/model'

const props = defineProps<{
  contracts: Contract[]
  picked: string[]
  problems: boolean
  /** 亮著的形狀數與總數。 */
  lit: number
  total: number
}>()

const emit = defineEmits<{
  (e: 'update:picked', value: string[]): void
  (e: 'update:problems', value: boolean): void
}>()

const filtering = computed(() => props.picked.length > 0 || props.problems)
const broken = computed(() => props.contracts.filter((c) => c.problems).length)

function toggle(id: string) {
  const next = props.picked.includes(id)
    ? props.picked.filter((x) => x !== id)
    : [...props.picked, id]
  emit('update:picked', next)
}
</script>

<template>
  <aside class="panel">
    <label class="check lead">
      <input
        type="checkbox" :checked="problems"
        @change="emit('update:problems', ($event.target as HTMLInputElement).checked)"
      >
      只讓有問題的亮著
      <span v-if="broken" class="badge">{{ broken }}</span>
    </label>

    <div class="head">
      <span class="muted">連線契約</span>
      <button v-if="picked.length" class="link" @click="emit('update:picked', [])">全部取消</button>
    </div>

    <ul class="list">
      <li v-for="c in contracts" :key="c.relationship">
        <label class="check">
          <input type="checkbox" :checked="picked.includes(c.relationship)" @change="toggle(c.relationship)">
          <span class="name" :title="c.label">{{ c.label }}</span>
          <!-- 連線數 0 = 這個環境沒實現它。這是最該被看見的一種，
               所以它不但留在清單裡，還要標出來。 -->
          <span v-if="c.connections === 0" class="tag none">沒實現</span>
          <span v-else class="tag">{{ c.connections }}</span>
          <span v-if="c.problems" class="dot" title="有 lint 叫過" />
        </label>
      </li>
    </ul>

    <p class="muted foot">
      <template v-if="filtering">
        <!-- 「亮著」不是「顯示」。圖上的東西一個都沒少，只是暗下去了。 -->
        <strong>{{ lit }}</strong> / {{ total }} 個形狀亮著。
        其他的還在圖上，只是調暗了——所以這張圖仍然可以拿來對帳。
      </template>
      <template v-else>
        全部亮著。勾幾條契約，或勾上面那個，其他就會淡下去。
      </template>
    </p>
  </aside>
</template>

<style scoped>
.panel {
  width: 240px;
  flex: none;
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px;
  border-right: 1px solid var(--rule);
  background: var(--surface-2);
  font-size: 12.5px;
  min-height: 0;
}

.lead { padding-bottom: 8px; border-bottom: 1px solid var(--rule); }

.head { display: flex; align-items: baseline; gap: 8px; }
.head .muted { flex: 1; font-size: 11.5px; }

.list { flex: 1; min-height: 0; overflow-y: auto; margin: 0; padding: 0; list-style: none; }
.list li + li { margin-top: 2px; }

.check { display: flex; align-items: center; gap: 6px; cursor: pointer; }
.check input { accent-color: var(--warp); flex: none; }
.name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.tag { flex: none; font-size: 11px; color: var(--ink-3); font-variant-numeric: tabular-nums; }
.tag.none { color: var(--broken); }

.dot { flex: none; width: 6px; height: 6px; border-radius: 50%; background: var(--broken); }

.badge {
  padding: 0 5px;
  border-radius: 8px;
  background: color-mix(in srgb, var(--broken) 18%, transparent);
  color: var(--broken);
  font-size: 11px;
}

.link { background: none; border: 0; padding: 0; color: var(--warp); cursor: pointer; font-size: 11.5px; }

.foot {
  padding-top: 8px;
  border-top: 1px solid var(--rule);
  font-size: 11.5px;
  line-height: 1.6;
}
</style>
