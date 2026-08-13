<script setup lang="ts">
/**
 * 會打字的下拉選單。
 *
 * # 為什麼不用原生的 `<select>`
 *
 * 這個專案的選單天生會很長：一個環境幾百台服務實體，一個外部系統十幾個接點。
 * 原生選單只能靠捲，而使用者手上通常已經有名字了——他要的是「打三個字就到」。
 *
 * # 兩條規矩
 *
 * 1. **失焦時絕不安靜地清空。** 打了一半沒選中就走掉，要退回原本選的那個。
 *    悄悄變空是這個工具最不能出的那種錯——使用者以為填好了，其實沒有。
 * 2. **找不到就直說。** 空的清單跟「壞掉了」長得一樣。
 */
import { computed, nextTick, ref, watch } from 'vue'

export interface Option {
  value: string
  label: string
  /** 分組的標題。同一組的要排在一起。 */
  group?: string
  /** 副標，畫在名字後面（位址、種類）。也一起參與搜尋。 */
  hint?: string
}

const props = withDefaults(
  defineProps<{
    modelValue: string | null
    options: Option[]
    placeholder?: string
    /** 允許「不選」。給那些真的可以空白的欄位用。 */
    allowEmpty?: boolean
    emptyLabel?: string
    disabled?: boolean
  }>(),
  { placeholder: '打字搜尋…', allowEmpty: false, emptyLabel: '（不指定）', disabled: false },
)

const emit = defineEmits<{ (e: 'update:modelValue', value: string | null): void }>()

const input = ref<HTMLInputElement | null>(null)
const list = ref<HTMLElement | null>(null)
const open = ref(false)
const query = ref('')
const active = ref(0)

const selected = computed(() => props.options.find((o) => o.value === props.modelValue) ?? null)

/** 沒展開時顯示選中的那個；展開時顯示使用者打的字。 */
const text = computed(() => (open.value ? query.value : (selected.value?.label ?? '')))

const matches = computed<Option[]>(() => {
  const needle = query.value.trim().toLowerCase()
  if (!needle) return props.options
  return props.options.filter((o) =>
    `${o.label} ${o.hint ?? ''} ${o.group ?? ''}`.toLowerCase().includes(needle),
  )
})

/** 分組畫出來。沒有 group 的就全部在同一段，不畫標題。 */
const groups = computed(() => {
  const out = new Map<string, Option[]>()
  for (const o of matches.value) {
    const key = o.group ?? ''
    if (!out.has(key)) out.set(key, [])
    out.get(key)!.push(o)
  }
  return [...out.entries()]
})

/** 鍵盤上下鍵走的是攤平之後的順序，跟畫面看到的一致。 */
const flat = computed(() => groups.value.flatMap(([, items]) => items))

function show() {
  if (props.disabled) return
  open.value = true
  query.value = ''
  active.value = Math.max(0, flat.value.findIndex((o) => o.value === props.modelValue))
  void nextTick(() => input.value?.select())
}

function choose(option: Option | null) {
  emit('update:modelValue', option?.value ?? null)
  open.value = false
  query.value = ''
}

/**
 * 收起來。**不改值**——打了一半沒選中就走掉，要退回原本選的那個。
 * 悄悄變空是這個工具最不能出的那種錯。
 */
function dismiss() {
  open.value = false
  query.value = ''
}

function onKeydown(e: KeyboardEvent) {
  if (!open.value) {
    if (e.key === 'ArrowDown' || e.key === 'Enter') { show(); e.preventDefault() }
    return
  }
  if (e.key === 'ArrowDown') {
    active.value = Math.min(active.value + 1, flat.value.length - 1)
    e.preventDefault()
  } else if (e.key === 'ArrowUp') {
    active.value = Math.max(active.value - 1, 0)
    e.preventDefault()
  } else if (e.key === 'Enter') {
    const pick = flat.value[active.value]
    if (pick) choose(pick)
    e.preventDefault()
  } else if (e.key === 'Escape') {
    dismiss()
    e.preventDefault()
  } else if (e.key === 'Tab') {
    dismiss()
  }
}

// 打字之後把游標拉回第一項——否則按 Enter 選到的是上一輪停留的位置。
watch(query, () => { active.value = 0 })

watch(active, () => {
  void nextTick(() => {
    list.value?.querySelector('[data-active="true"]')?.scrollIntoView({ block: 'nearest' })
  })
})

function indexOf(option: Option) {
  return flat.value.indexOf(option)
}
</script>

<template>
  <div class="picker" :class="{ disabled }">
    <input
      ref="input"
      role="combobox"
      :aria-expanded="open"
      aria-autocomplete="list"
      :value="text"
      :placeholder="selected ? selected.label : placeholder"
      :disabled="disabled"
      class="mono"
      @focus="show()"
      @input="query = ($event.target as HTMLInputElement).value; open = true"
      @keydown="onKeydown"
      @blur="dismiss()"
    >
    <span class="caret" aria-hidden="true">▾</span>

    <!-- `mousedown.prevent` 讓點選單不會先觸發 blur 把清單關掉。 -->
    <div v-if="open" ref="list" class="menu" role="listbox" @mousedown.prevent>
      <button
        v-if="allowEmpty"
        class="item empty"
        role="option"
        :aria-selected="modelValue === null"
        @click="choose(null)"
      >{{ emptyLabel }}</button>

      <template v-for="[name, items] in groups" :key="name">
        <div v-if="name" class="group">{{ name }}</div>
        <button
          v-for="o in items" :key="o.value"
          class="item"
          role="option"
          :aria-selected="o.value === modelValue"
          :data-active="indexOf(o) === active"
          @click="choose(o)"
        >
          <span class="label">{{ o.label }}</span>
          <span v-if="o.hint" class="hint">{{ o.hint }}</span>
        </button>
      </template>

      <!-- 空的清單跟「壞掉了」長得一樣，所以要說話。 -->
      <p v-if="!matches.length" class="none muted">
        沒有符合「{{ query }}」的項目。
      </p>
    </div>
  </div>
</template>

<style scoped>
.picker { position: relative; display: flex; align-items: center; }
.picker.disabled { opacity: 0.55; }

input { width: 100%; padding-right: 20px; }
.caret {
  position: absolute;
  right: 6px;
  font-size: 9px;
  color: var(--ink-3);
  pointer-events: none;
}

.menu {
  position: absolute;
  top: calc(100% + 2px);
  left: 0;
  right: 0;
  z-index: 30;
  max-height: 260px;
  overflow-y: auto;
  padding: 3px;
  border: 1px solid var(--rule);
  border-radius: 5px;
  background: var(--surface);
  box-shadow: 0 6px 20px color-mix(in srgb, #000 25%, transparent);
}

.group {
  padding: 5px 8px 3px;
  font-size: 10.5px;
  color: var(--ink-3);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.item {
  display: flex;
  align-items: baseline;
  gap: 8px;
  width: 100%;
  padding: 4px 8px;
  border: 0;
  border-radius: 4px;
  background: none;
  text-align: left;
  font-size: 12.5px;
  cursor: pointer;
}
.item:hover { background: var(--surface-2); }
.item[data-active='true'] { background: color-mix(in srgb, var(--warp) 16%, var(--surface)); }
.item[aria-selected='true'] .label { font-weight: 600; }
.item .label { flex: 1; font-family: var(--mono, monospace); overflow: hidden; text-overflow: ellipsis; }
.item .hint { flex: none; font-size: 11px; color: var(--ink-3); }
.item.empty { color: var(--ink-3); font-style: italic; }

.none { margin: 0; padding: 8px; font-size: 11.5px; }
</style>
