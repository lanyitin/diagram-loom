<script setup lang="ts">
/**
 * 挑要比對哪幾個環境。
 *
 * 沒勾＝全部。只有一個環境時才隱藏——那時真的沒有東西可比。
 *
 * 原本的門檻訂在三個，理由是「兩個環境勾選沒意義」。那是錯的：
 * 兩欄的時候「我現在只想看 prod」一樣是真實需求，尤其契約很多的時候。
 */
import { useProject } from '../lib/store'
import type { Id } from '../lib/model'

const store = useProject()

function toggle(id: Id) {
  // 空陣列的意思是「全部」，所以第一次點某個環境時，
  // 要先把其他環境展開成明確的清單，否則會變成「只剩沒點的那些」。
  if (store.selectedEnvironments.length === 0) {
    store.selectedEnvironments = store.environments.map((e) => e.id).filter((e) => e !== id)
    return
  }
  const selected = new Set(store.selectedEnvironments)
  if (selected.has(id)) {
    selected.delete(id)
  } else {
    selected.add(id)
  }
  // 全部都勾＝沒有在篩選，回到空陣列這個表示法。
  store.selectedEnvironments = selected.size === store.environments.length ? [] : [...selected]
}

function anySelected(id: Id) {
  return store.selectedEnvironments.length === 0 || store.selectedEnvironments.includes(id)
}
</script>

<template>
  <div v-if="store.environments.length >= 2" class="picker">
    <span class="muted label">比對</span>
    <label v-for="env in store.environments" :key="env.id" class="chip" :class="{ on: anySelected(env.id) }">
      <input type="checkbox" :checked="anySelected(env.id)" @change="toggle(env.id)">
      {{ env.slug }}
    </label>
    <button v-if="store.selectedEnvironments.length" class="reset" @click="store.selectedEnvironments = []">
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
