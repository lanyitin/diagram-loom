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
import Picker from './Picker.vue'
import type { Id, Resource } from '../lib/model'

const store = useProject()

/** 表單編輯的複本。直接改 store 裡的那份會讓畫面在按取消之後回不去。 */
const draft = ref<Resource | null>(null)

watch(
  () => store.editingResource,
  (pending) => {
    draft.value = pending ? (JSON.parse(JSON.stringify(pending.resource)) as Resource) : null
  },
  { immediate: true },
)

/** 這個環境裡所有的機器，攤平成一串（含站點底下的）。落地要選一台。 */
const machines = computed(() => {
  const envId = (draft.value as { instance?: { environment: Id } })?.instance?.environment
  const env = store.environments.find((e) => e.id === envId)
  const out: { value: string; label: string; hint?: string }[] = []
  const walk = (nodes: { id: Id; slug: string; kind: string; children?: unknown[] }[], path: string) => {
    for (const n of nodes) {
      out.push({ value: n.id, label: path ? `${path} / ${n.slug}` : n.slug, hint: kindLabel(n.kind) })
      walk((n.children ?? []) as never, path ? `${path} / ${n.slug}` : n.slug)
    }
  }
  walk((env?.nodes ?? []) as never, '')
  return out
})

function kindLabel(kind: string): string {
  return { site: '站點', physical: '實體機', 'virtual-machine': '虛擬機', 'linux-container': 'Linux 容器' }[kind] ?? kind
}

/**
 * 落地的接點。**IP 與 port 就填在這裡。**
 *
 * `def` 指向邏輯層的接點定義——填了之後 lint 才知道「這台的 client-port
 * 對應到契約上的哪一個」。沒填也可以，只是那條連線接不上。
 */
const instanceEndpoints = computed(() => {
  const inst = (draft.value as { instance?: { instance: { endpoints?: unknown[] } } })?.instance?.instance
  return (inst?.endpoints ?? []) as { id: Id; slug: string; def: Id | null; protocol: string; address: string | null }[]
})

/** 這個落地對應的服務上，定義了哪些接點。 */
const defsOfContainer = computed(() => {
  const inst = (draft.value as { instance?: { instance: { container: Id } } })?.instance?.instance
  const c = containers.value.find((x) => x.id === inst?.container)
  return (c?.endpoints ?? []).map((d) => ({ value: d.id, label: d.slug, hint: d.protocol }))
})

function addEndpoint() {
  const inst = (draft.value as { instance?: { instance: { endpoints: unknown[] } } })?.instance?.instance
  if (!inst) return
  inst.endpoints = [
    ...(inst.endpoints ?? []),
    { id: crypto.randomUUID(), slug: '', def: null, protocol: 'tcp', address: null },
  ]
}

function removeEndpoint(id: Id) {
  const inst = (draft.value as { instance?: { instance: { endpoints: { id: Id }[] } } })?.instance?.instance
  if (!inst) return
  inst.endpoints = inst.endpoints.filter((e) => e.id !== id)
}

const logical = computed(() => store.snapshot?.project.logical)
const containers = computed(() => logical.value?.containers ?? [])
const systems = computed(() => logical.value?.systems ?? [])
const people = computed(() => logical.value?.people ?? [])

/** 契約的一端：三種來源攤成一個選單，選了就換掉那一端的形狀。 */
const endChoices = computed(() => [
  ...people.value.map((p) => ({ v: `person:${p.id}`, label: `人：${p.slug}` })),
  ...containers.value.map((c) => ({ v: `container:${c.id}`, label: `服務：${c.slug}` })),
  ...systems.value.map((s) => ({ v: `system:${s.id}`, label: `系統：${s.slug}` })),
])

function endValue(end: unknown): string {
  const e = end as Record<string, Id>
  if (e?.person) return `person:${e.person}`
  if (e?.container) return `container:${e.container}`
  if (e?.system) return `system:${e.system}`
  return ''
}

function setEnd(which: 'from' | 'to', v: string) {
  const [kind, id] = v.split(':')
  const rel = (draft.value as { relationship?: Record<string, unknown> })?.relationship
  if (!rel || !id) return
  rel[which] = { [kind!]: id }
}

/** 契約的 `to_endpoint` 只能挑目標身上的接點定義——那是 L012 在管的。 */
const targetEndpoints = computed(() => {
  const rel = (draft.value as { relationship?: Record<string, Record<string, Id>> })?.relationship
  const to = rel?.to
  if (!to) return []
  if (to.container) return containers.value.find((c) => c.id === to.container)?.endpoints ?? []
  if (to.system) return systems.value.find((s) => s.id === to.system)?.endpoints ?? []
  return []
})

/** 可以掛在底下的既有節點。攤平成一層，縮排表示層級。 */
const placeableNodes = computed(() => {
  const envId = (draft.value as { node?: { environment: Id } })?.node?.environment
  const env = store.environments.find((e) => e.id === envId)
  const out: { id: Id; label: string }[] = []
  const walk = (nodes: { id: Id; slug: string; children?: unknown[] }[], depth: number) => {
    for (const n of nodes) {
      out.push({ id: n.id, label: `${'　'.repeat(depth)}${n.slug}` })
      walk((n.children ?? []) as typeof nodes, depth + 1)
    }
  }
  walk((env?.nodes ?? []) as never, 0)
  return out
})

async function submit() {
  const pending = store.editingResource
  const r = draft.value
  if (!pending || !r) return
  store.editingResource = null
  await store.applyEdit(pending.isNew ? { addResource: r } : { updateResource: r })
}

/** `v-model` 綁巢狀的 optional 欄位很吵，用兩個小工具收掉。 */
function read(path: string): string {
  return path.split('.').reduce<never>((o, k) => (o as never)?.[k], draft.value as never) ?? ''
}
function write(path: string, v: unknown) {
  const keys = path.split('.')
  const last = keys.pop()!
  const o = keys.reduce<never>((o, k) => (o as never)?.[k], draft.value as never)
  if (o) (o as Record<string, unknown>)[last] = v
}
</script>

<template>
  <div v-if="store.editingResource && draft" class="scrim" @click.self="store.editingResource = null">
    <section class="box" role="dialog" aria-modal="true">
      <h2>{{ store.editingResource.isNew ? '新增' : '編輯' }}{{ store.editingResource.kind }}</h2>

      <!-- 人 -->
      <template v-if="'person' in draft">
        <label>名稱<input :value="read('person.slug')" class="mono" @input="write('person.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>顯示名<input :value="read('person.name')" @input="write('person.name', ($event.target as HTMLInputElement).value)"></label>
      </template>

      <!-- 系統 -->
      <template v-else-if="'system' in draft">
        <label>名稱<input :value="read('system.slug')" class="mono" @input="write('system.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>顯示名<input :value="read('system.name')" @input="write('system.name', ($event.target as HTMLInputElement).value)"></label>
        <label class="row">
          <input type="checkbox" :checked="!!read('system.external')" @change="write('system.external', ($event.target as HTMLInputElement).checked)">
          外部系統（不是我們自己部署的）
        </label>
      </template>

      <!-- 服務 -->
      <template v-else-if="'container' in draft">
        <label>名稱<input :value="read('container.slug')" class="mono" @input="write('container.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>顯示名<input :value="read('container.name')" @input="write('container.name', ($event.target as HTMLInputElement).value)"></label>
        <label>所屬系統
          <select :value="read('container.system')" @change="write('container.system', ($event.target as HTMLSelectElement).value)">
            <option value="">（尚未選擇）</option>
            <option v-for="s in systems" :key="s.id" :value="s.id">{{ s.slug }}</option>
          </select>
        </label>
      </template>

      <!-- 接點定義 -->
      <template v-else-if="'endpointDef' in draft">
        <label>掛在誰身上
          <select :value="read('endpointDef.owner')" @change="write('endpointDef.owner', ($event.target as HTMLSelectElement).value)">
            <option value="">（尚未選擇）</option>
            <optgroup label="服務">
              <option v-for="c in containers" :key="c.id" :value="c.id">{{ c.slug }}</option>
            </optgroup>
            <optgroup label="外部系統">
              <option v-for="s in systems" :key="s.id" :value="s.id">{{ s.slug }}</option>
            </optgroup>
          </select>
        </label>
        <label>名稱<input :value="read('endpointDef.def.slug')" class="mono" @input="write('endpointDef.def.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>協定
          <select :value="read('endpointDef.def.protocol')" @change="write('endpointDef.def.protocol', ($event.target as HTMLSelectElement).value)">
            <option value="tcp">TCP</option>
            <option value="udp">UDP</option>
            <option value="unix-socket">Unix socket</option>
            <option value="jdbc">JDBC</option>
            <option value="file">檔案</option>
          </select>
        </label>
      </template>

      <!-- 契約 -->
      <template v-else-if="'relationship' in draft">
        <label>名稱<input :value="read('relationship.slug')" class="mono" @input="write('relationship.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>用途<input :value="read('relationship.purpose')" placeholder="這條連線是做什麼用的" @input="write('relationship.purpose', ($event.target as HTMLInputElement).value)"></label>
        <label>來源
          <select :value="endValue(draft.relationship?.from)" @change="setEnd('from', ($event.target as HTMLSelectElement).value)">
            <option value="">（尚未選擇）</option>
            <option v-for="o in endChoices" :key="o.v" :value="o.v">{{ o.label }}</option>
          </select>
        </label>
        <label>目標
          <select :value="endValue(draft.relationship?.to)" @change="setEnd('to', ($event.target as HTMLSelectElement).value)">
            <option value="">（尚未選擇）</option>
            <option v-for="o in endChoices" :key="o.v" :value="o.v">{{ o.label }}</option>
          </select>
        </label>
        <label>連到目標的哪個接點
          <select :value="read('relationship.toEndpoint')" @change="write('relationship.toEndpoint', ($event.target as HTMLSelectElement).value)">
            <option value="">（尚未選擇）</option>
            <option v-for="e in targetEndpoints" :key="e.id" :value="e.id">{{ e.slug }}</option>
          </select>
        </label>
        <p v-if="targetEndpoints.length === 0" class="muted hint">
          目標身上還沒有任何接點定義。要先去「接點定義」那一頁建一個。
        </p>
      </template>

      <!-- 環境 -->
      <template v-else-if="'environment' in draft">
        <label>名稱<input :value="read('environment.slug')" class="mono" placeholder="prod" @input="write('environment.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>顯示名<input :value="read('environment.name')" placeholder="正式環境" @input="write('environment.name', ($event.target as HTMLInputElement).value)"></label>
      </template>

      <!-- 機器 -->
      <template v-else-if="'node' in draft">
        <label>名稱<input :value="read('node.node.slug')" class="mono" @input="write('node.node.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>種類
          <select :value="read('node.node.kind')" @change="write('node.node.kind', ($event.target as HTMLSelectElement).value)">
            <option value="site">站點（裝別的機器用的）</option>
            <option value="physical">實體機</option>
            <option value="virtual-machine">虛擬機</option>
            <option value="linux-container">Linux 容器</option>
          </select>
        </label>
        <label v-if="store.editingResource.isNew">放在哪個節點底下
          <select :value="read('node.within')" @change="write('node.within', ($event.target as HTMLSelectElement).value || null)">
            <option value="">（環境最上層）</option>
            <option v-for="n in placeableNodes" :key="n.id" :value="n.id">{{ n.label }}</option>
          </select>
        </label>
      </template>

      <!-- 設備 -->
      <template v-else-if="'infra' in draft">
        <label>名稱<input :value="read('infra.node.slug')" class="mono" placeholder="f5-01" @input="write('infra.node.slug', ($event.target as HTMLInputElement).value)"></label>
        <p class="muted hint">VIP 位址要另外在「設備」那一列上加。</p>
      </template>

      <!-- 設備接點 -->
      <template v-else-if="'infraEndpoint' in draft">
        <label>名稱<input :value="read('infraEndpoint.endpoint.slug')" class="mono" placeholder="vip-redis" @input="write('infraEndpoint.endpoint.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>位址<input :value="read('infraEndpoint.endpoint.address')" class="mono" placeholder="10.0.0.100:6379" @input="write('infraEndpoint.endpoint.address', ($event.target as HTMLInputElement).value || null)"></label>
      </template>

      <!-- 落地：位址就住在這裡 -->
      <template v-else-if="'instance' in draft">
        <label>名稱<input :value="read('instance.instance.slug')" class="mono" placeholder="redis-01" @input="write('instance.instance.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>哪個服務
          <Picker
            :model-value="read('instance.instance.container') || null"
            :options="containers.map((c) => ({ value: c.id, label: c.slug, hint: c.name }))"
            placeholder="打字搜尋服務…"
            @update:model-value="write('instance.instance.container', $event)"
          />
        </label>
        <label>跑在哪台機器上
          <Picker
            :model-value="read('instance.node') || null"
            :options="machines"
            placeholder="打字搜尋機器…"
            @update:model-value="write('instance.node', $event)"
          />
        </label>

        <!-- 接點：IP 與 port 在這裡。之前只能等 L006 叫了才改得到。 -->
        <div class="sub">
          <div class="subhead">
            <strong>接點與位址</strong>
            <button type="button" class="link" @click="addEndpoint()">＋ 加一個</button>
          </div>
          <p v-if="!instanceEndpoints.length" class="muted hint">
            還沒有接點。<strong>IP 與 port 填在接點上</strong>——一個服務可以有好幾個
            （對外一個、管理介面一個）。
          </p>
          <div v-for="e in instanceEndpoints" :key="e.id" class="ep">
            <input v-model="e.slug" class="mono" placeholder="client-port">
            <input v-model="e.address" class="mono" placeholder="10.0.1.11:6379">
            <Picker
              :model-value="e.def"
              :options="defsOfContainer"
              allow-empty
              empty-label="（沒對應到契約）"
              placeholder="對應哪個接點定義…"
              @update:model-value="e.def = $event"
            />
            <button type="button" class="icon del" title="拿掉這個接點" @click="removeEndpoint(e.id)">✕</button>
          </div>
        </div>

        <label class="check">
          <input type="checkbox" :checked="!!read('instance.instance.standalone')" @change="write('instance.instance.standalone', ($event.target as HTMLInputElement).checked)">
          刻意獨立（沒有任何連線碰到它也不要叫）
        </label>
      </template>

      <!-- 外部系統落地 -->
      <template v-else-if="'systemInstance' in draft">
        <label>名稱<input :value="read('systemInstance.instance.slug')" class="mono" @input="write('systemInstance.instance.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>對應哪個外部系統
          <select :value="read('systemInstance.instance.system')" @change="write('systemInstance.instance.system', ($event.target as HTMLSelectElement).value)">
            <option value="">（尚未選擇）</option>
            <option v-for="s in systems" :key="s.id" :value="s.id">{{ s.slug }}</option>
          </select>
        </label>
      </template>

      <footer>
        <span class="muted hint">按錯了可以按 ⌘Z 復原</span>
        <span class="grow" />
        <button @click="store.editingResource = null">取消</button>
        <button class="primary" @click="submit()">{{ store.editingResource.isNew ? '建立' : '儲存' }}</button>
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
  max-height: 86%;
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

.sub {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 10px;
  border: 1px solid var(--rule);
  border-radius: 5px;
  background: var(--surface-2);
}
.subhead { display: flex; align-items: baseline; gap: 8px; font-size: 12.5px; }
.subhead strong { flex: 1; }
.ep { display: grid; grid-template-columns: 1fr 1.3fr 1.2fr auto; gap: 6px; align-items: center; }
.link { background: none; border: 0; padding: 0; color: var(--warp); cursor: pointer; font-size: 11.5px; }
.check { display: flex; flex-direction: row; align-items: center; gap: 6px; }
.check input { accent-color: var(--warp); flex: none; }
</style>
