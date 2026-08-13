<script setup lang="ts">
/**
 * 新增／編輯一個資源。
 *
 * # 這裡的欄位排版是前端的事，規則不是
 *
 * 「名字不能空白」「同一層不能重名」「接點要屬於那個服務」都在 Rust
 * （`resource::add` / `update` 與 L012）。這裡送出去就好，
 * 錯了會拿到一句話，原樣顯示。
 *
 * 若哪天這裡開始出現 `if (已經有一個叫這個的)`，那就是規則漏到前端了。
 *
 * # 為什麼直接改一份複本
 *
 * `Resource` 是完整的值，表單改的就是它。送出時整個送回去，
 * Rust 那邊決定哪些欄位吃、哪些不動（例如改環境名字時不碰裡面的機器）。
 */
import { computed, ref, watch } from 'vue'
import { useProject } from '../lib/store'
import type { Id, Resource } from '../lib/model'

const store = useProject()

/** 表單編輯的複本。直接改 store 裡的那份會讓畫面在按取消之後回不去。 */
const 草稿 = ref<Resource | null>(null)

watch(
  () => store.編輯資源中,
  (要編的) => {
    草稿.value = 要編的 ? (JSON.parse(JSON.stringify(要編的.resource)) as Resource) : null
  },
  { immediate: true },
)

const 邏輯 = computed(() => store.snapshot?.project.logical)
const 服務們 = computed(() => 邏輯.value?.containers ?? [])
const 系統們 = computed(() => 邏輯.value?.systems ?? [])
const 人們 = computed(() => 邏輯.value?.people ?? [])

/** 契約的一端：三種來源攤成一個選單，選了就換掉那一端的形狀。 */
const 端選項 = computed(() => [
  ...人們.value.map((p) => ({ v: `person:${p.id}`, label: `人：${p.slug}` })),
  ...服務們.value.map((c) => ({ v: `container:${c.id}`, label: `服務：${c.slug}` })),
  ...系統們.value.map((s) => ({ v: `system:${s.id}`, label: `系統：${s.slug}` })),
])

function 端的值(end: unknown): string {
  const e = end as Record<string, Id>
  if (e?.person) return `person:${e.person}`
  if (e?.container) return `container:${e.container}`
  if (e?.system) return `system:${e.system}`
  return ''
}

function 設端(which: 'from' | 'to', v: string) {
  const [kind, id] = v.split(':')
  const rel = (草稿.value as { relationship?: Record<string, unknown> })?.relationship
  if (!rel || !id) return
  rel[which] = { [kind!]: id }
}

/** 契約的 `to_endpoint` 只能挑目標身上的接點定義——那是 L012 在管的。 */
const 目標的接點 = computed(() => {
  const rel = (草稿.value as { relationship?: Record<string, Record<string, Id>> })?.relationship
  const to = rel?.to
  if (!to) return []
  if (to.container) return 服務們.value.find((c) => c.id === to.container)?.endpoints ?? []
  if (to.system) return 系統們.value.find((s) => s.id === to.system)?.endpoints ?? []
  return []
})

/** 可以掛在底下的既有節點。攤平成一層，縮排表示層級。 */
const 可放的節點 = computed(() => {
  const envId = (草稿.value as { node?: { environment: Id } })?.node?.environment
  const env = store.環境.find((e) => e.id === envId)
  const out: { id: Id; label: string }[] = []
  const 走 = (nodes: { id: Id; slug: string; children?: unknown[] }[], 深: number) => {
    for (const n of nodes) {
      out.push({ id: n.id, label: `${'　'.repeat(深)}${n.slug}` })
      走((n.children ?? []) as typeof nodes, 深 + 1)
    }
  }
  走((env?.nodes ?? []) as never, 0)
  return out
})

async function 送出() {
  const 要編的 = store.編輯資源中
  const r = 草稿.value
  if (!要編的 || !r) return
  store.編輯資源中 = null
  await store.套用編輯(要編的.新的 ? { addResource: r } : { updateResource: r })
}

/** `v-model` 綁巢狀的 optional 欄位很吵，用兩個小工具收掉。 */
function 取(路徑: string): string {
  return 路徑.split('.').reduce<never>((o, k) => (o as never)?.[k], 草稿.value as never) ?? ''
}
function 設(路徑: string, v: unknown) {
  const keys = 路徑.split('.')
  const last = keys.pop()!
  const o = keys.reduce<never>((o, k) => (o as never)?.[k], 草稿.value as never)
  if (o) (o as Record<string, unknown>)[last] = v
}
</script>

<template>
  <div v-if="store.編輯資源中 && 草稿" class="scrim" @click.self="store.編輯資源中 = null">
    <section class="box" role="dialog" aria-modal="true">
      <h2>{{ store.編輯資源中.新的 ? '新增' : '編輯' }}{{ store.編輯資源中.kind }}</h2>

      <!-- 人 -->
      <template v-if="'person' in 草稿">
        <label>名稱<input :value="取('person.slug')" class="mono" @input="設('person.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>顯示名<input :value="取('person.name')" @input="設('person.name', ($event.target as HTMLInputElement).value)"></label>
      </template>

      <!-- 系統 -->
      <template v-else-if="'system' in 草稿">
        <label>名稱<input :value="取('system.slug')" class="mono" @input="設('system.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>顯示名<input :value="取('system.name')" @input="設('system.name', ($event.target as HTMLInputElement).value)"></label>
        <label class="row">
          <input type="checkbox" :checked="!!取('system.external')" @change="設('system.external', ($event.target as HTMLInputElement).checked)">
          外部系統（不是我們自己部署的）
        </label>
      </template>

      <!-- 服務 -->
      <template v-else-if="'container' in 草稿">
        <label>名稱<input :value="取('container.slug')" class="mono" @input="設('container.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>顯示名<input :value="取('container.name')" @input="設('container.name', ($event.target as HTMLInputElement).value)"></label>
        <label>所屬系統
          <select :value="取('container.system')" @change="設('container.system', ($event.target as HTMLSelectElement).value)">
            <option value="">（尚未選擇）</option>
            <option v-for="s in 系統們" :key="s.id" :value="s.id">{{ s.slug }}</option>
          </select>
        </label>
      </template>

      <!-- 接點定義 -->
      <template v-else-if="'endpointDef' in 草稿">
        <label>掛在誰身上
          <select :value="取('endpointDef.owner')" @change="設('endpointDef.owner', ($event.target as HTMLSelectElement).value)">
            <option value="">（尚未選擇）</option>
            <optgroup label="服務">
              <option v-for="c in 服務們" :key="c.id" :value="c.id">{{ c.slug }}</option>
            </optgroup>
            <optgroup label="外部系統">
              <option v-for="s in 系統們" :key="s.id" :value="s.id">{{ s.slug }}</option>
            </optgroup>
          </select>
        </label>
        <label>名稱<input :value="取('endpointDef.def.slug')" class="mono" @input="設('endpointDef.def.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>協定
          <select :value="取('endpointDef.def.protocol')" @change="設('endpointDef.def.protocol', ($event.target as HTMLSelectElement).value)">
            <option value="tcp">TCP</option>
            <option value="udp">UDP</option>
            <option value="unix-socket">Unix socket</option>
            <option value="jdbc">JDBC</option>
            <option value="file">檔案</option>
          </select>
        </label>
      </template>

      <!-- 契約 -->
      <template v-else-if="'relationship' in 草稿">
        <label>名稱<input :value="取('relationship.slug')" class="mono" @input="設('relationship.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>用途<input :value="取('relationship.purpose')" placeholder="這條連線是做什麼用的" @input="設('relationship.purpose', ($event.target as HTMLInputElement).value)"></label>
        <label>來源
          <select :value="端的值(草稿.relationship?.from)" @change="設端('from', ($event.target as HTMLSelectElement).value)">
            <option value="">（尚未選擇）</option>
            <option v-for="o in 端選項" :key="o.v" :value="o.v">{{ o.label }}</option>
          </select>
        </label>
        <label>目標
          <select :value="端的值(草稿.relationship?.to)" @change="設端('to', ($event.target as HTMLSelectElement).value)">
            <option value="">（尚未選擇）</option>
            <option v-for="o in 端選項" :key="o.v" :value="o.v">{{ o.label }}</option>
          </select>
        </label>
        <label>連到目標的哪個接點
          <select :value="取('relationship.toEndpoint')" @change="設('relationship.toEndpoint', ($event.target as HTMLSelectElement).value)">
            <option value="">（尚未選擇）</option>
            <option v-for="e in 目標的接點" :key="e.id" :value="e.id">{{ e.slug }}</option>
          </select>
        </label>
        <p v-if="目標的接點.length === 0" class="muted hint">
          目標身上還沒有任何接點定義。要先去「接點定義」那一頁建一個。
        </p>
      </template>

      <!-- 環境 -->
      <template v-else-if="'environment' in 草稿">
        <label>名稱<input :value="取('environment.slug')" class="mono" placeholder="prod" @input="設('environment.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>顯示名<input :value="取('environment.name')" placeholder="正式環境" @input="設('environment.name', ($event.target as HTMLInputElement).value)"></label>
      </template>

      <!-- 機器 -->
      <template v-else-if="'node' in 草稿">
        <label>名稱<input :value="取('node.node.slug')" class="mono" @input="設('node.node.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>種類
          <select :value="取('node.node.kind')" @change="設('node.node.kind', ($event.target as HTMLSelectElement).value)">
            <option value="site">站點（裝別的機器用的）</option>
            <option value="physical">實體機</option>
            <option value="virtual-machine">虛擬機</option>
            <option value="linux-container">Linux 容器</option>
          </select>
        </label>
        <label v-if="store.編輯資源中.新的">放在哪個節點底下
          <select :value="取('node.within')" @change="設('node.within', ($event.target as HTMLSelectElement).value || null)">
            <option value="">（環境最上層）</option>
            <option v-for="n in 可放的節點" :key="n.id" :value="n.id">{{ n.label }}</option>
          </select>
        </label>
      </template>

      <!-- 設備 -->
      <template v-else-if="'infra' in 草稿">
        <label>名稱<input :value="取('infra.node.slug')" class="mono" placeholder="f5-01" @input="設('infra.node.slug', ($event.target as HTMLInputElement).value)"></label>
        <p class="muted hint">VIP 位址要另外在「設備」那一列上加。</p>
      </template>

      <!-- 設備接點 -->
      <template v-else-if="'infraEndpoint' in 草稿">
        <label>名稱<input :value="取('infraEndpoint.endpoint.slug')" class="mono" placeholder="vip-redis" @input="設('infraEndpoint.endpoint.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>位址<input :value="取('infraEndpoint.endpoint.address')" class="mono" placeholder="10.0.0.100:6379" @input="設('infraEndpoint.endpoint.address', ($event.target as HTMLInputElement).value || null)"></label>
      </template>

      <!-- 外部系統落地 -->
      <template v-else-if="'systemInstance' in 草稿">
        <label>名稱<input :value="取('systemInstance.instance.slug')" class="mono" @input="設('systemInstance.instance.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>對應哪個外部系統
          <select :value="取('systemInstance.instance.system')" @change="設('systemInstance.instance.system', ($event.target as HTMLSelectElement).value)">
            <option value="">（尚未選擇）</option>
            <option v-for="s in 系統們" :key="s.id" :value="s.id">{{ s.slug }}</option>
          </select>
        </label>
      </template>

      <footer>
        <span class="muted hint">按錯了可以按 ⌘Z 復原</span>
        <span class="grow" />
        <button @click="store.編輯資源中 = null">取消</button>
        <button class="primary" @click="送出()">{{ store.編輯資源中.新的 ? '建立' : '儲存' }}</button>
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
  width: min(520px, 92vw);
  max-height: 86vh;
  overflow: auto;
  padding: 18px 20px 14px;
  border: 1px solid var(--rule);
  border-radius: 8px;
  background: var(--surface);
  display: flex;
  flex-direction: column;
  gap: 10px;
}
h2 { margin: 0; font-size: 15px; font-weight: 600; }
p { margin: 0; }

label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
  color: var(--ink-2);
}
label.row { flex-direction: row; align-items: center; gap: 6px; }
label.row input { accent-color: var(--warp); }

.hint { font-size: 11.5px; line-height: 1.6; }

footer { display: flex; align-items: center; gap: 8px; margin-top: 6px; }
.grow { flex: 1; }
</style>
