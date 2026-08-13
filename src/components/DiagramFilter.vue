<script setup lang="ts">
/**
 * 圖的篩選。做成**浮動的**，不是側邊欄。
 *
 * # 為什麼不佔一欄
 *
 * draw.io 自己就有兩片面板（左邊形狀庫、右邊格式）要佔位置。我們再站一欄，
 * 畫布只剩中間一條——而且擠到它的面板就等於**把編輯器弄到不能操作**。
 *
 * 所以關起來的時候**一個像素都不佔**，打開才蓋在畫布上，關掉就還回去。
 * draw.io 自己的 plugin 也是這樣做的（`plugins/props.js` 開一個浮動視窗）。
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
  (e: 'close'): void
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
  <div class="card" role="dialog" aria-label="篩選">
    <div class="top">
      <strong class="title">篩選</strong>
      <button v-if="filtering" class="link" @click="emit('update:picked', []); emit('update:problems', false)">
        全部取消
      </button>
      <button class="x" aria-label="關閉" @click="emit('close')">✕</button>
    </div>

    <label class="check lead">
      <input
        type="checkbox" :checked="problems"
        @change="emit('update:problems', ($event.target as HTMLInputElement).checked)"
      >
      只讓有問題的亮著
      <span v-if="broken" class="badge">{{ broken }}</span>
    </label>

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
      <li v-if="!contracts.length" class="muted small">這個專案還沒有連線契約。</li>
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
  </div>
</template>

<style scoped>
.card {
  position: absolute;
  top: 6px;
  left: 8px;
  z-index: 5;
  width: 260px;
  max-height: calc(100% - 20px);
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px;
  border: 1px solid var(--rule);
  border-radius: 6px;
  background: var(--surface);
  box-shadow: 0 6px 20px color-mix(in srgb, #000 22%, transparent);
  font-size: 12.5px;
}

.top { display: flex; align-items: center; gap: 8px; }
.title { flex: 1; font-size: 12.5px; }
.x { background: none; border: 0; padding: 0 2px; color: var(--ink-3); cursor: pointer; font-size: 12px; }

.lead { padding-bottom: 8px; border-bottom: 1px solid var(--rule); }
.small { font-size: 11.5px; }

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
