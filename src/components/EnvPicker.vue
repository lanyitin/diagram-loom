<script setup lang="ts">
/**
 * 挑要比對哪幾個環境。
 *
 * 沒勾＝全部。環境不多時勾選沒意義，所以少於三個就整個不顯示——
 * 畫面上不放不影響任何事情的控制項。
 */
import { useProject } from '../lib/store'
import type { Id } from '../lib/model'

const store = useProject()

function 切換(id: Id) {
  // 空陣列的意思是「全部」，所以第一次點某個環境時，
  // 要先把其他環境展開成明確的清單，否則會變成「只剩沒點的那些」。
  if (store.比對中的環境.length === 0) {
    store.比對中的環境 = store.環境.map((e) => e.id).filter((e) => e !== id)
    return
  }
  const 已選 = new Set(store.比對中的環境)
  if (已選.has(id)) {
    已選.delete(id)
  } else {
    已選.add(id)
  }
  // 全部都勾＝沒有在篩選，回到空陣列這個表示法。
  store.比對中的環境 = 已選.size === store.環境.length ? [] : [...已選]
}

function 有勾(id: Id) {
  return store.比對中的環境.length === 0 || store.比對中的環境.includes(id)
}
</script>

<template>
  <div v-if="store.環境.length >= 3" class="picker">
    <span class="muted label">比對</span>
    <label v-for="env in store.環境" :key="env.id" class="chip" :class="{ on: 有勾(env.id) }">
      <input type="checkbox" :checked="有勾(env.id)" @change="切換(env.id)">
      {{ env.slug }}
    </label>
    <button v-if="store.比對中的環境.length" class="reset" @click="store.比對中的環境 = []">
      全選
    </button>
  </div>
</template>

<style scoped>
.picker { display: flex; align-items: center; gap: 6px; }
.label { font-size: 11px; letter-spacing: .06em; text-transform: uppercase; }

.chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 9px;
  border: 1px solid var(--rule);
  border-radius: 5px;
  font-family: var(--mono);
  font-size: 12px;
  color: var(--ink-3);
  background: var(--surface);
}

.chip.on {
  color: var(--ink);
  border-color: color-mix(in srgb, var(--warp) 55%, transparent);
  background: color-mix(in srgb, var(--warp) 8%, transparent);
}

.chip input { margin: 0; accent-color: var(--warp); }

.reset { padding: 3px 9px; font-size: 12px; }
</style>
