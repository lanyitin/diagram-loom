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

const proposal = ref<Proposal | null>(null)
const fromChoiceList = ref<Choice[]>([])
const toChoiceList = ref<Choice[]>([])
const fromChoices = ref<Endpointing | null>(null)
const target = ref<Endpointing | null>(null)
const purpose = ref('')
const isFallback = ref(false)
const computing = ref(false)

/** 下拉選單依 group 分段，找起來比一長串快。 */
function groupBy(all: Choice[]) {
  const g = new Map<string, Choice[]>()
  for (const c of all) {
    if (!g.has(c.group)) g.set(c.group, [])
    g.get(c.group)!.push(c)
  }
  return [...g.entries()]
}

const fromGroups = computed(() => groupBy(fromChoiceList.value))
const toGroups = computed(() => groupBy(toChoiceList.value))

/** 選單的 value 用序號——`endpointing` 是物件，塞不進 `<option value>`。 */
function picked(all: Choice[], i: string): Endpointing | null {
  return all[Number(i)]?.endpointing ?? null
}

function indexOf(all: Choice[], current: Endpointing | null): string {
  if (!current) return ''
  const i = all.findIndex((c) => JSON.stringify(c.endpointing) === JSON.stringify(current))
  return i < 0 ? '' : String(i)
}

watch(
  () => store.addingConnection,
  async (pending) => {
    proposal.value = null
    fromChoices.value = null
    target.value = null
    purpose.value = ''
    isFallback.value = false
    if (!pending) return

    computing.value = true
    const [p, f, t] = await Promise.all([
      commands.proposeConnection(pending.environment, pending.relationship),
      commands.connectionChoices(pending.environment, 'from'),
      commands.connectionChoices(pending.environment, 'to'),
    ])
    computing.value = false

    if (p.status !== 'ok') {
      store.error = (p.error as { message?: string })?.message ?? String(p.error)
      store.addingConnection = null
      return
    }
    proposal.value = p.data
    purpose.value = p.data.purpose
    fromChoices.value = p.data.from
    target.value = p.data.to
    if (f.status === 'ok') fromChoiceList.value = f.data
    if (t.status === 'ok') toChoiceList.value = t.data
  },
  { immediate: true },
)

const canCreate = computed(() => fromChoices.value !== null && target.value !== null)

async function create() {
  const pending = store.addingConnection
  const p = proposal.value
  if (!pending || !p || !fromChoices.value || !target.value) return

  store.addingConnection = null
  await store.applyEdit({
    addConnection: {
      environment: pending.environment,
      id: p.id,
      serves: p.serves,
      purpose: purpose.value,
      kind: isFallback.value ? 'fallback' : 'primary',
      from: fromChoices.value,
      to: target.value,
    },
  })
}
</script>

<template>
  <div v-if="store.addingConnection" class="scrim" @click.self="store.addingConnection = null">
    <section class="box" role="dialog" aria-modal="true">
      <h2>補一條連線</h2>
      <p class="muted sub">
        <span class="mono">{{ store.addingConnection.label }}</span>
        ・{{ store.envName(store.addingConnection.environment) }}
      </p>

      <p v-if="computing" class="muted">正在照契約擬一條…</p>

      <template v-else-if="proposal">
        <!-- 工具做了什麼假設、哪裡擬不出來，一定要講。
             擬得像真的卻不說，使用者會直接按下去。 -->
        <ul v-if="proposal.notes.length" class="notes">
          <li v-for="(n, i) in proposal.notes" :key="i">{{ n }}</li>
        </ul>

        <label class="field">
          <span>來源</span>
          <select :value="indexOf(fromChoiceList, fromChoices)" @change="fromChoices = picked(fromChoiceList, ($event.target as HTMLSelectElement).value)">
            <option value="">（尚未選擇）</option>
            <optgroup v-for="[g, items] in fromGroups" :key="g" :label="g">
              <option v-for="c in items" :key="c.label" :value="fromChoiceList.indexOf(c)">{{ c.label }}</option>
            </optgroup>
          </select>
        </label>

        <label class="field">
          <span>目標</span>
          <select :value="indexOf(toChoiceList, target)" @change="target = picked(toChoiceList, ($event.target as HTMLSelectElement).value)">
            <option value="">（尚未選擇）</option>
            <optgroup v-for="[g, items] in toGroups" :key="g" :label="g">
              <option v-for="c in items" :key="c.label" :value="toChoiceList.indexOf(c)">{{ c.label }}</option>
            </optgroup>
          </select>
        </label>

        <label class="field">
          <span>用途</span>
          <input v-model="purpose" type="text" placeholder="這條連線是做什麼用的">
        </label>

        <label class="toggle">
          <input v-model="isFallback" type="checkbox">
          這是備援路徑（只在故障時走）
        </label>
      </template>

      <footer>
        <span class="muted hint">建錯了可以按 ⌘Z 復原</span>
        <span class="grow" />
        <button @click="store.addingConnection = null">取消</button>
        <button class="primary" :disabled="!canCreate || computing" @click="create()">建立</button>
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
  max-height: calc(84vh / var(--zoom));
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
