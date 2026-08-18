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
import type { Id, Kind, Resource } from '../lib/model'

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

/** 這個環境裡所有的機器，攤平成一串（含站點底下的）。服務實體要選一台。 */
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

interface EndpointDraft {
  id: Id
  slug: string
  def: Id | null
  protocol: string
  address: string | null
}

/**
 * 正在編輯的那個實體——服務實體或外部系統實體。
 *
 * # 為什麼兩種要走同一條路
 *
 * **IP 與 port 住在接點上，兩種實體都一樣。** 原本只有服務實體有這段編輯
 * 介面，於是外部系統實體的清單有一欄「位址」，卻沒有任何畫面填得了它——
 * 而 lint 會為此叫（L001「外部系統在這個環境沒有指定位址」），
 * 一個指著問題卻沒給任何辦法的工具比不叫還糟。
 *
 * 差別只在 `def` 該從誰身上找：服務實體看它對應的服務，外部系統實體看
 * 它對應的那個外部系統（見 `docs/domain-model.md` 的不對稱表）。
 */
/** 兩種實體共通的部分：接點住在這裡，`standalone` 也是。 */
interface InstanceHolder {
  endpoints?: EndpointDraft[]
  standalone: boolean
}

const editingInstance = computed<{ holder: InstanceHolder; owner: Id } | null>(() => {
  const d = draft.value as {
    instance?: { instance: InstanceHolder & { container: Id } }
    systemInstance?: { instance: InstanceHolder & { system: Id } }
  } | null
  if (d?.instance) return { holder: d.instance.instance, owner: d.instance.instance.container }
  if (d?.systemInstance) {
    return { holder: d.systemInstance.instance, owner: d.systemInstance.instance.system }
  }
  return null
})

/**
 * 接點。**IP 與 port 就填在這裡。**
 *
 * `def` 指向邏輯層的接點定義——填了之後 lint 才知道「這台的 client-port
 * 對應到契約上的哪一個」。沒填也可以，只是那條連線接不上。
 */
const instanceEndpoints = computed<EndpointDraft[]>(
  () => (editingInstance.value?.holder.endpoints ?? []) as EndpointDraft[],
)

/** 這個實體對應的服務或外部系統上，定義了哪些接點。 */
const defsOfOwner = computed(() => {
  const owner = editingInstance.value?.owner
  const defs =
    containers.value.find((c) => c.id === owner)?.endpoints
    ?? systems.value.find((s) => s.id === owner)?.endpoints
    ?? []
  return defs.map((d) => ({ value: d.id, label: d.slug, hint: d.protocol }))
})

function addEndpoint() {
  const holder = editingInstance.value?.holder
  if (!holder) return
  holder.endpoints = [
    ...(holder.endpoints ?? []),
    { id: crypto.randomUUID(), slug: '', def: null, protocol: 'tcp', address: null },
  ]
}

/**
 * 挑了對應的接點定義。
 *
 * **協定要跟著換。** `addEndpoint` 產生的接點一律先是 `tcp`，因為那時候還
 * 不知道要對應到誰；挑好之後不同步的話，一個對應到 JDBC 定義的接點會一直
 * 記著 `tcp`——存下來的資料跟它自己指的定義不一致。
 *
 * 這件事沒有畫面看得出來（協定沒有欄位可以編，也不進 lint），只有匯入
 * 試算表時的差異比對會印出來——所以它會安靜地錯很久。
 *
 * 名字則只在**還空著**的時候才代填。使用者打過的東西不要動。
 */
function pickDef(endpoint: EndpointDraft, def: Id | null) {
  endpoint.def = def
  const chosen = defsOfOwner.value.find((d) => d.value === def)
  if (!chosen) return
  endpoint.protocol = chosen.hint ?? endpoint.protocol
  if (!endpoint.slug.trim()) endpoint.slug = chosen.label
}

function removeEndpoint(id: Id) {
  const holder = editingInstance.value?.holder
  if (!holder) return
  holder.endpoints = (holder.endpoints ?? []).filter((e) => e.id !== id)
}

const logical = computed(() => store.snapshot?.project.logical)
const containers = computed(() => logical.value?.containers ?? [])
const systems = computed(() => logical.value?.systems ?? [])

/**
 * 外部系統實體只能對應到**外部**系統。
 *
 * 自家系統是靠自己的 `Container` 部署的，沒有獨立的系統實例
 * （見 `loom_core::logical::Logical::external_systems` 的說明）。
 * 列出來的話使用者建得出一個模型上不該存在的東西，而 lint 不會叫——
 * L001 只走 `external_systems()`，根本不會看到它。
 */
const externalSystems = computed(() =>
  systems.value
    .filter((s) => s.external)
    .map((s) => ({ value: s.id, label: s.slug, hint: s.name })),
)
const people = computed(() => logical.value?.people ?? [])

/**
 * 全部系統，標明自家還是外部。
 *
 * 這裡**不篩掉外部系統**：Rust 沒有禁止服務掛在外部系統底下
 * （L012 只檢查那個 id 存在）。文件說「C4 不拆外部系統」是設計意圖，
 * 但把意圖偷偷實作成 UI 的篩選，就是在前端養第二套規則——
 * 而且既有專案裡真的有這種資料時，畫面會顯示成一片空白。
 *
 * 所以做法是**標出來讓人看得見**，不是替他決定。
 */
const allSystems = computed(() =>
  systems.value.map((s) => ({
    value: s.id,
    label: s.slug,
    hint: s.external ? '外部' : '自家',
  })),
)

/** 接點定義掛在服務或系統身上（`resource.rs` 兩邊都收）。 */
const endpointOwners = computed(() => [
  ...containers.value.map((c) => ({ value: c.id, label: c.slug, group: '服務', hint: c.name })),
  ...systems.value.map((s) => ({
    value: s.id,
    label: s.slug,
    group: '系統',
    hint: s.external ? '外部' : '自家',
  })),
])

/**
 * 契約的一端：三種來源攤成一個選單，選了就換掉那一端的形狀。
 *
 * 「人」兩端都列得出來，**這裡不自己擋**——擋掉等於前端多一套規則。
 * 規則在 Rust，而且是兩層：`resource::write_into` 在建立與修改時直接拒絕
 * （`EditError::PersonAsTarget`），L013 則負責既有資料與手改的 YAML。
 *
 * 所以使用者真的選了「人」當目標時，會拿到 Rust 給的那句話，
 * 而不是一個安靜地不能按的選項。
 */
const endChoices = computed(() => [
  ...people.value.map((p) => ({ value: `person:${p.id}`, label: p.slug, group: '人' })),
  ...containers.value.map((c) => ({
    value: `container:${c.id}`,
    label: c.slug,
    group: '服務',
    hint: c.name,
  })),
  ...systems.value.map((s) => ({
    value: `system:${s.id}`,
    label: s.slug,
    group: '系統',
    hint: s.external ? '外部' : '自家',
  })),
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

/**
 * 每一種資源的備註住在哪。
 *
 * # 為什麼是一張表，不是十一個分支各放一個 `<label>`
 *
 * 抄十一份的話，下一個新增的資源種類一定會漏掉一份——而漏掉的症狀是
 * 「表格上看得到、MCP 寫得進去，只有人在畫面上填不了」：沒有錯誤訊息、
 * 沒有規則會叫，只有使用者自己發現。**那正是這一欄要補的東西本身。**
 *
 * 對照的是 Rust 的 `Resource::memo()`——那邊也是攤開一次就好。
 *
 * 型別寫成 `Record<Kind, string>`：`Kind` 是 bindings 從 Rust 的
 * `resource::Kind` 產的，多一種資源時這裡少一個鍵就編不過。
 */
const MEMO_PATHS: Record<Kind, string> = {
  person: 'person.memo',
  system: 'system.memo',
  container: 'container.memo',
  endpointDef: 'endpointDef.def.memo',
  relationship: 'relationship.memo',
  environment: 'environment.memo',
  node: 'node.node.memo',
  infra: 'infra.node.memo',
  infraEndpoint: 'infraEndpoint.endpoint.memo',
  instance: 'instance.instance.memo',
  systemInstance: 'systemInstance.instance.memo',
}

/** 手上這份 draft 的備註在哪。`Resource` 是單鍵的聯集，物件上只會有一把鑰匙。 */
const memoPath = computed(() => {
  const d = draft.value
  if (!d) return undefined
  const kind = (Object.keys(MEMO_PATHS) as Kind[]).find((k) => k in d)
  return kind ? MEMO_PATHS[kind] : undefined
})

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
          <Picker
            :model-value="read('container.system') || null"
            :options="allSystems"
            placeholder="打字搜尋系統…"
            @update:model-value="write('container.system', $event)"
          />
        </label>
      </template>

      <!-- 接點定義 -->
      <template v-else-if="'endpointDef' in draft">
        <label>掛在誰身上
          <Picker
            :model-value="read('endpointDef.owner') || null"
            :options="endpointOwners"
            placeholder="打字搜尋服務或系統…"
            @update:model-value="write('endpointDef.owner', $event)"
          />
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
          <Picker
            :model-value="endValue(draft.relationship?.from) || null"
            :options="endChoices"
            placeholder="打字搜尋人、服務或系統…"
            @update:model-value="setEnd('from', $event ?? '')"
          />
        </label>
        <label>目標
          <Picker
            :model-value="endValue(draft.relationship?.to) || null"
            :options="endChoices"
            placeholder="打字搜尋人、服務或系統…"
            @update:model-value="setEnd('to', $event ?? '')"
          />
        </label>
        <label>連到目標的哪個接點
          <Picker
            :model-value="read('relationship.toEndpoint') || null"
            :options="targetEndpoints.map((e) => ({ value: e.id, label: e.slug, hint: e.protocol }))"
            placeholder="打字搜尋接點定義…"
            @update:model-value="write('relationship.toEndpoint', $event)"
          />
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
          <Picker
            :model-value="read('node.within') || null"
            :options="placeableNodes.map((n) => ({ value: n.id, label: n.label }))"
            allow-empty
            empty-label="（環境最上層）"
            placeholder="打字搜尋機器…"
            @update:model-value="write('node.within', $event)"
          />
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

      <!-- 服務實體：位址就住在這裡 -->
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
      </template>

      <!-- 外部系統實體 -->
      <template v-else-if="'systemInstance' in draft">
        <label>名稱<input :value="read('systemInstance.instance.slug')" class="mono" @input="write('systemInstance.instance.slug', ($event.target as HTMLInputElement).value)"></label>
        <label>對應哪個外部系統
          <Picker
            :model-value="read('systemInstance.instance.system') || null"
            :options="externalSystems"
            placeholder="打字搜尋外部系統…"
            @update:model-value="write('systemInstance.instance.system', $event)"
          />
        </label>
        <!-- 一個外部系統都還沒建的話，這個表單填不出東西來。
             空的選單跟「壞掉了」長得一樣，所以要說清楚下一步在哪。 -->
        <p v-if="!externalSystems.length" class="muted hint">
          還沒有<strong>外部系統</strong>可以對應。先到「系統」那一頁建一個並勾起
          <strong>外部</strong>——自家系統是靠自己的服務部署的，沒有獨立的系統實例。
        </p>
      </template>

      <!--
        接點與位址、刻意獨立：**兩種實體共用**。

        IP 與 port 住在接點上，服務實體與外部系統實體都一樣。原本只有服務
        實體有這一段，於是外部系統實體的清單有一欄「位址」卻沒有任何畫面
        填得了它——而 lint 會為此叫（L001「外部系統在這個環境沒有指定位址」）。
        工具指著問題卻沒給任何辦法，比不叫還糟。

        放在 if/else 鏈之後而不是抄兩份：抄的話遲早只有一邊會被修到。
      -->
      <template v-if="editingInstance">
        <div class="sub">
          <div class="subhead">
            <strong>接點與位址</strong>
            <button type="button" class="link" @click="addEndpoint()">＋ 加一個</button>
          </div>
          <p v-if="!instanceEndpoints.length" class="muted hint">
            還沒有接點。<strong>IP 與 port 填在接點上</strong>——一個東西可以有好幾個
            （對外一個、管理介面一個）。
          </p>
          <div v-for="e in instanceEndpoints" :key="e.id" class="ep">
            <input v-model="e.slug" class="mono" placeholder="client-port">
            <input v-model="e.address" class="mono" placeholder="10.0.1.11:6379">
            <Picker
              :model-value="e.def"
              :options="defsOfOwner"
              allow-empty
              empty-label="（沒對應到契約）"
              placeholder="對應哪個接點定義…"
              @update:model-value="pickDef(e, $event)"
            />
            <button type="button" class="icon del" title="拿掉這個接點" @click="removeEndpoint(e.id)">✕</button>
          </div>
        </div>

        <label class="check">
          <input
            type="checkbox"
            :checked="editingInstance.holder.standalone"
            @change="editingInstance.holder.standalone = ($event.target as HTMLInputElement).checked"
          >
          刻意獨立（沒有任何連線碰到它也不要叫）
        </label>
      </template>

      <!--
        備註：**十一種資源都有**，所以放在 if/else 鏈之後寫一次。

        抄十一份的話遲早只有幾份會被修到，而漏掉的那一種症狀是「表格上看得到、
        MCP 寫得進去、人填不了」——完全沒有錯誤訊息。理由同上面「接點與位址」那段。

        放最後：它是自由文字、長度不受控，不該把有結構的欄位擠出畫面。
        跟 `inventory::tables` 把備註放每張表的最後一欄是同一個決定。
      -->
      <label v-if="memoPath">備註
        <textarea
          :value="read(memoPath)"
          rows="3"
          placeholder="規則管不到的話寫在這裡"
          @input="write(memoPath, ($event.target as HTMLTextAreaElement).value)"
        />
      </label>

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
/*
 * 接點的一列：名字、位址、對應的接點定義、拿掉。
 *
 * # `minmax(0, …)` 不能省
 *
 * `fr` 的自動最小值是 min-content，而 `<input>` 的 min-content 是它的
 * `size`（預設 20 個字）。所以前兩欄會硬撐到各約 180px，把第三欄擠成
 * **一條縫**——那個 Picker 會變成一個按不到的細長方塊，它的選單跟著只有
 * 一個字寬，選項一個字一行往下排。
 *
 * 症狀看起來像 Picker 壞了，其實是這一行沒寫 `minmax(0, …)`。
 */
.ep {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1.3fr) minmax(0, 1.2fr) auto;
  gap: 6px;
  align-items: center;
}
/* 格線放行之後，格子裡的東西也要肯縮。 */
.ep > * { min-width: 0; }
/*
 * 備註是這張表單裡唯一的多行輸入。
 *
 * `user-select: text` 不能省：`styles.css` 對 body 下了 `user-select: none`
 * （這是桌面應用不是網頁），而它只替 `input[type=text]` 解開。少了這一行，
 * 使用者**選不到自己剛打的字**——複製貼上、雙擊選字全部失效，
 * 而畫面上看起來完全正常。
 */
textarea { font: inherit; user-select: text; resize: vertical; }

.link { background: none; border: 0; padding: 0; color: var(--warp); cursor: pointer; font-size: 11.5px; }
.check { display: flex; flex-direction: row; align-items: center; gap: 6px; }
.check input { accent-color: var(--warp); flex: none; }
</style>
