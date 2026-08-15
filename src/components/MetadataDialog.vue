<script setup lang="ts">
/**
 * 編輯資料（draw.io 的 Edit Data，⌘M）。
 *
 * # 為什麼這個對這個專案特別重要
 *
 * draw.io 的自訂屬性就是**我們的 `loomId` 住的地方**。所以它不是一個
 * 「進階使用者才用得到」的對話框——它是綁定機制的手動入口，也是使用者
 * 看得到「這個形狀到底代表誰」的唯一地方。
 *
 * `loomId` / `loomKind` 標成保留鍵：**改得動，但畫面要說它是什麼**。
 * 鎖死的話，使用者遇到我們沒想到的情況就完全沒有出路。
 */
import { ref } from 'vue'
import type { Cell, Graph } from '@maxgraph/core'

const props = defineProps<{ graph: Graph; cell: Cell }>()
const emit = defineEmits<{ (e: 'close'): void; (e: 'change'): void }>()

/** 這幾個鍵是機器在用的。 */
const RESERVED = ['loomId', 'loomKind']

/** 值換成 XML 元素才放得下自訂屬性——draw.io 按「編輯資料」時做的也是這件事。 */
function asElement(cell: Cell): Element {
  const value = cell.getValue() as unknown
  if (value && typeof value === 'object' && 'getAttribute' in (value as object)) {
    return (value as Element).cloneNode(true) as Element
  }
  const doc = new DOMParser().parseFromString('<object/>', 'text/xml')
  doc.documentElement.setAttribute('label', value == null ? '' : String(value))
  return doc.documentElement
}

const element = asElement(props.cell)
const rows = ref(
  element.getAttributeNames().map((name) => ({ name, value: element.getAttribute(name) ?? '' })),
)

function add() {
  let name = '屬性'
  let n = 1
  while (rows.value.some((r) => r.name === name)) name = `屬性${n += 1}`
  rows.value.push({ name, value: '' })
}

function apply() {
  const doc = new DOMParser().parseFromString('<object/>', 'text/xml')
  const next = doc.documentElement
  for (const row of rows.value) {
    const name = row.name.trim()
    if (name) next.setAttribute(name, row.value)
  }
  props.graph.batchUpdate(() => props.graph.model.setValue(props.cell, next))
  emit('change')
  emit('close')
}
</script>

<template>
  <!-- 點外面關掉、Esc 關掉。少了這兩個會讓人覺得被困住。 -->
  <div class="modal" @click.self="emit('close')" @keydown.esc="emit('close')">
    <div class="dialog" role="dialog" aria-label="編輯資料">
      <h2>編輯資料</h2>

      <div v-for="(row, i) in rows" :key="i" class="metarow" :class="{ reserved: RESERVED.includes(row.name) }">
        <input
          v-model="row.name" class="key" :readonly="RESERVED.includes(row.name)"
          :title="RESERVED.includes(row.name) ? '這個鍵是綁定用的，改名會讓對帳看不見它' : ''"
        >
        <input v-model="row.value" class="value">
        <button class="x" title="刪掉這一項" @click="rows.splice(i, 1)">✕</button>
      </div>

      <button class="add" @click="add()">＋ 新增一項</button>

      <p v-if="rows.some((r) => r.name === 'loomId')" class="hint muted">
        <strong>loomId</strong> 是這個形狀代表哪個模型元素。改掉它，對帳看到的就是另一個東西。
      </p>

      <div class="foot">
        <button @click="emit('close')">取消</button>
        <button class="primary" @click="apply()">套用</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.modal {
  position: fixed; inset: 0; z-index: 50;
  display: grid; place-items: center;
  background: color-mix(in srgb, #000 28%, transparent);
}
.dialog {
  width: 420px;
  /* ⚠️ 不用 `vh`：那個會不會被介面縮放放大是未知的，而未知的東西不該放在
     版面骨架上（見 `styles.test.ts`）。百分比是相對於這個 grid 容器的。 */
  max-height: 76%;
  overflow-y: auto;
  padding: 14px;
  border-radius: 8px; background: var(--surface);
  box-shadow: 0 12px 40px color-mix(in srgb, #000 30%, transparent);
}
h2 { margin: 0 0 10px; font-size: 13px; }

.metarow { display: flex; gap: 5px; margin-bottom: 5px; }
.metarow .key { width: 120px; }
.metarow .value { flex: 1; min-width: 0; }
.metarow.reserved .key { background: var(--surface-2); color: var(--warp); font-weight: 600; }
.x { width: 26px; cursor: pointer; color: var(--ink-3); }

.add {
  margin-top: 6px; padding: 4px 8px;
  border: 1px dashed var(--rule); border-radius: 4px; background: none;
  cursor: pointer; font-size: 12px; color: var(--ink-3);
}

.hint { margin: 10px 0 0; font-size: 11.5px; line-height: 1.6; }
.foot { display: flex; justify-content: flex-end; gap: 6px; margin-top: 14px; }
.foot .primary { background: var(--warp); border-color: var(--warp); color: #fff; }
</style>
