<script setup lang="ts">
/**
 * 新增連線：L001／L002 的修法。
 *
 * # 為什麼是「看過提案再改」，不是「填一張空白表單」
 *
 * L001 的意思是**母版說這裡該有一條，而這個環境沒有**。母版上已經寫了
 * 誰連誰、連到哪個接點，缺的只是「在這個環境對應到哪幾台機器」——那查得出來。
 * 所以先由 Rust 擬一份（`propose_connection`），使用者確認或換掉某一端就好。
 *
 * # 提案的假設一定要顯示出來
 *
 * 工具不知道中間要不要經過 F5——那是人的決定。擬得像真的卻不說，
 * 使用者會直接按下去，然後少掉一段，而少一段正是這工具要抓的東西。
 *
 * # 這裡沒有任何模型知識
 *
 * 兩個下拉選單的內容來自 `connection_choices`，每一項帶著一個
 * **不透明的 `endpointing`**，原樣送回去就能建。前端不需要知道
 * Instance 跟萬用字元有什麼差別，也不需要知道人為什麼不能當目標。
 */
import { computed, ref, watch } from 'vue'
import { commands } from '../lib/bindings'
import { useProject } from '../lib/store'
import type { Choice, Endpointing, Proposal } from '../lib/model'

const store = useProject()

const 提案 = ref<Proposal | null>(null)
const 來源選項 = ref<Choice[]>([])
const 目標選項 = ref<Choice[]>([])
const 來源 = ref<Endpointing | null>(null)
const 目標 = ref<Endpointing | null>(null)
const 用途 = ref('')
const 備援 = ref(false)
const 算著 = ref(false)

/** 下拉選單依 group 分段，找起來比一長串快。 */
function 分組(全部: Choice[]) {
  const g = new Map<string, Choice[]>()
  for (const c of 全部) {
    if (!g.has(c.group)) g.set(c.group, [])
    g.get(c.group)!.push(c)
  }
  return [...g.entries()]
}

const 來源分組 = computed(() => 分組(來源選項.value))
const 目標分組 = computed(() => 分組(目標選項.value))

/** 選單的 value 用序號——`endpointing` 是物件，塞不進 `<option value>`。 */
function 選了(全部: Choice[], i: string): Endpointing | null {
  return 全部[Number(i)]?.endpointing ?? null
}

function 序號(全部: Choice[], 現在: Endpointing | null): string {
  if (!現在) return ''
  const i = 全部.findIndex((c) => JSON.stringify(c.endpointing) === JSON.stringify(現在))
  return i < 0 ? '' : String(i)
}

watch(
  () => store.新增連線中,
  async (目標契約) => {
    提案.value = null
    來源.value = null
    目標.value = null
    用途.value = ''
    備援.value = false
    if (!目標契約) return

    算著.value = true
    const [p, f, t] = await Promise.all([
      commands.proposeConnection(目標契約.environment, 目標契約.relationship),
      commands.connectionChoices(目標契約.environment, 'from'),
      commands.connectionChoices(目標契約.environment, 'to'),
    ])
    算著.value = false

    if (p.status !== 'ok') {
      store.錯誤 = (p.error as { message?: string })?.message ?? String(p.error)
      store.新增連線中 = null
      return
    }
    提案.value = p.data
    用途.value = p.data.purpose
    來源.value = p.data.from
    目標.value = p.data.to
    if (f.status === 'ok') 來源選項.value = f.data
    if (t.status === 'ok') 目標選項.value = t.data
  },
  { immediate: true },
)

const 可以建了 = computed(() => 來源.value !== null && 目標.value !== null)

async function 建立() {
  const 目標契約 = store.新增連線中
  const p = 提案.value
  if (!目標契約 || !p || !來源.value || !目標.value) return

  store.新增連線中 = null
  await store.套用編輯({
    addConnection: {
      environment: 目標契約.environment,
      id: p.id,
      serves: p.serves,
      purpose: 用途.value,
      kind: 備援.value ? 'fallback' : 'primary',
      from: 來源.value,
      to: 目標.value,
    },
  })
}
</script>

<template>
  <div v-if="store.新增連線中" class="scrim" @click.self="store.新增連線中 = null">
    <section class="box" role="dialog" aria-modal="true">
      <h2>補一條連線</h2>
      <p class="muted sub">
        <span class="mono">{{ store.新增連線中.label }}</span>
        ・{{ store.環境名(store.新增連線中.environment) }}
      </p>

      <p v-if="算著" class="muted">正在照契約擬一條…</p>

      <template v-else-if="提案">
        <!-- 工具做了什麼假設、哪裡擬不出來，一定要講。
             擬得像真的卻不說，使用者會直接按下去。 -->
        <ul v-if="提案.notes.length" class="notes">
          <li v-for="(n, i) in 提案.notes" :key="i">{{ n }}</li>
        </ul>

        <label class="field">
          <span>來源</span>
          <select :value="序號(來源選項, 來源)" @change="來源 = 選了(來源選項, ($event.target as HTMLSelectElement).value)">
            <option value="">（尚未選擇）</option>
            <optgroup v-for="[g, items] in 來源分組" :key="g" :label="g">
              <option v-for="c in items" :key="c.label" :value="來源選項.indexOf(c)">{{ c.label }}</option>
            </optgroup>
          </select>
        </label>

        <label class="field">
          <span>目標</span>
          <select :value="序號(目標選項, 目標)" @change="目標 = 選了(目標選項, ($event.target as HTMLSelectElement).value)">
            <option value="">（尚未選擇）</option>
            <optgroup v-for="[g, items] in 目標分組" :key="g" :label="g">
              <option v-for="c in items" :key="c.label" :value="目標選項.indexOf(c)">{{ c.label }}</option>
            </optgroup>
          </select>
        </label>

        <label class="field">
          <span>用途</span>
          <input v-model="用途" type="text" placeholder="這條連線是做什麼用的">
        </label>

        <label class="toggle">
          <input v-model="備援" type="checkbox">
          這是備援路徑（只在故障時走）
        </label>
      </template>

      <footer>
        <span class="muted hint">建錯了可以按 ⌘Z 復原</span>
        <span class="grow" />
        <button @click="store.新增連線中 = null">取消</button>
        <button class="primary" :disabled="!可以建了 || 算著" @click="建立()">建立</button>
      </footer>
    </section>
  </div>
</template>

<style scoped>
.scrim {
  position: fixed;
  inset: 0;
  display: grid;
  place-items: center;
  background: color-mix(in srgb, #000 42%, transparent);
  z-index: 20;
}
.box {
  width: min(620px, 92vw);
  max-height: 84vh;
  overflow: auto;
  padding: 18px 20px 14px;
  border: 1px solid var(--rule);
  border-radius: 8px;
  background: var(--surface);
  display: flex;
  flex-direction: column;
  gap: 12px;
}
h2 { margin: 0; font-size: 15px; font-weight: 600; }
p { margin: 0; }
.sub { font-size: 12.5px; }

.notes {
  margin: 0;
  padding: 9px 12px;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 5px;
  border: 1px solid color-mix(in srgb, var(--warn) 40%, transparent);
  border-radius: 5px;
  background: color-mix(in srgb, var(--warn) 8%, transparent);
  font-size: 12.5px;
  color: var(--ink-2);
}

.field { display: flex; align-items: center; gap: 10px; }
.field > span { width: 3.5em; flex: none; color: var(--ink-2); font-size: 12.5px; }
.field select, .field input { flex: 1; min-width: 0; }

.toggle { display: inline-flex; align-items: center; gap: 6px; color: var(--ink-2); font-size: 12.5px; }
.toggle input { accent-color: var(--warp); }

footer { display: flex; align-items: center; gap: 8px; margin-top: 4px; }
.grow { flex: 1; }
.hint { font-size: 11.5px; }
</style>
