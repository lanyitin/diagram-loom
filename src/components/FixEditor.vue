<script setup lang="ts">
/**
 * 就地修好一項發現的那一格。
 *
 * # 這裡沒有任何「哪條規則要填什麼」的判斷
 *
 * 控制項長什麼樣完全由 `finding.fix` 決定（Rust 的 `edit::fix_for` 算的），
 * 填完把值原封不動送回去，由 Rust 的 `edit_for` 決定要寫進哪個欄位。
 *
 * 所以新增一條 lint 規則時，**這個檔案不用改**。若哪天這裡開始出現
 * `if (rule === 'L006')`，那就是規則漏到前端了。
 */
import { onMounted, ref } from 'vue'
import { useProject } from '../lib/store'
import type { Finding, Fix, FixValue } from '../lib/model'

const props = defineProps<{ finding: Finding; fix: Fix }>()
const emit = defineEmits<{ done: [] }>()

const store = useProject()

/**
 * 使用者填的東西。
 *
 * 型別是 `string | number` 而不是 `string`：Vue 的 `v-model` 綁在
 * `<input type="number">` 上時會**自動轉成數字**，所以這裡拿到的不一定是字串。
 * 之前寫死成字串，`.trim()` 在數字上炸掉，按下「套用」什麼都沒發生。
 */
const draft = ref<string | number>(props.fix.text?.current ?? props.fix.count?.suggestion ?? '')
const inputEl = ref<HTMLInputElement | null>(null)

// 展開就把游標放進去。少一次點擊，使用者才願意一項一項修完。
onMounted(() => {
  inputEl.value?.focus()
  inputEl.value?.select()
})

async function submit() {
  const filled = String(draft.value).trim()
  let value: FixValue
  if (props.fix.text) {
    value = { text: filled }
  } else if (props.fix.count) {
    const n = Number(filled)
    // 留空 = 拿掉 expect。那是合法的（會讓 L005 重新叫），
    // 不是「什麼都沒填所以不要動」。
    value = { count: filled === '' || Number.isNaN(n) ? null : n }
  } else {
    value = { toggle: true }
  }
  emit('done')
  await store.applyFix(props.finding, value)
}
</script>

<template>
  <form @submit.prevent="submit" @keydown.esc="emit('done')">
    <input
      v-if="fix.text" ref="輸入框" v-model="draft"
      type="text" class="mono" :placeholder="fix.text.hint"
    >
    <template v-else-if="fix.count">
      <input ref="輸入框" v-model="draft" type="number" min="0" class="mono narrow">
      <span class="muted">實際符合 {{ fix.count.suggestion ?? '?' }} 個</span>
    </template>
    <span v-else-if="fix.toggle">{{ fix.toggle.label }}</span>

    <button class="primary" type="submit">{{ fix.toggle ? '標記' : '套用' }}</button>
  </form>
</template>

<style scoped>
form { display: flex; align-items: center; gap: 8px; }
input[type="text"] { flex: 1; max-width: 420px; }
.narrow { width: 84px; }
</style>
