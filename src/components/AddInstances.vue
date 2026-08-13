<script setup lang="ts">
/**
 * 批次建立機器：L001「這個服務一台都還沒建」的修法。
 *
 * # 為什麼一定要有
 *
 * 補連線的提案會說「目標的服務 redis 在 dev 一台都還沒建，要先建機器」。
 * 沒有這張表單，那句話就是死路——工具叫你去做一件它不讓你做的事。
 *
 * # 為什麼是樣板不是一台一台填
 *
 * 六台 Redis 手打六次會打錯，而打錯的是 IP，錯了不會有人發現。
 * `redis-{n}` + `10.0.1.{ip}` 讓「第幾台」與「第幾號 IP」各自遞增——
 * 現實中這兩個常常對不齊（`redis-01`～`redis-12` 對 `10.0.1.11`～`10.0.1.22`）。
 *
 * # 每改一個字就重算一次預覽
 *
 * 一次建六台是會後悔的操作。使用者要在按下去之前就看到那六行長什麼樣，
 * 而不是建完再回頭數。撞名也在這時候就講，不是建到一半才說。
 */
import { computed, ref, watch } from 'vue'
import { commands } from '../lib/bindings'
import { useProject } from '../lib/store'
import type { BatchPlan, BatchSpec, DeploymentNode, Id, NodeKind } from '../lib/model'

const store = useProject()

const 數量 = ref(3)
const 名稱樣板 = ref('')
const 機器樣板 = ref('')
const 起始 = ref(1)
const 補零 = ref(2)
const 位址樣板 = ref('')
const 位址起始 = ref(11)
const 機器種類 = ref<NodeKind>('virtual-machine')
const 接點 = ref<Id | null>(null)
const 放在 = ref<Id | null>(null)

const plan = ref<BatchPlan | null>(null)
const 擋下來的 = ref<string | null>(null)

/** 那個服務的接點定義。挑哪一個是使用者的事，但清單來自模型。 */
const 接點們 = computed(() => {
  const c = store.新增機器中?.container
  return store.snapshot?.project.logical.containers.find((x) => x.id === c)?.endpoints ?? []
})

/** 可以掛在底下的既有節點（通常是站點）。攤平成一層，縮排表示層級。 */
const 可放的節點 = computed(() => {
  const env = store.環境.find((e) => e.id === store.新增機器中?.environment)
  const out: { id: Id; label: string }[] = []
  const 走 = (nodes: DeploymentNode[], 深: number) => {
    for (const n of nodes) {
      out.push({ id: n.id, label: `${'　'.repeat(深)}${n.slug}` })
      走(n.children ?? [], 深 + 1)
    }
  }
  走(env?.nodes ?? [], 0)
  return out
})

watch(
  () => store.新增機器中,
  (要建的) => {
    plan.value = null
    擋下來的.value = null
    if (!要建的) return

    const slug = store.snapshot?.project.logical.containers
      .find((c) => c.id === 要建的.container)?.slug ?? 'node'
    數量.value = 3
    名稱樣板.value = `${slug}-{n}`
    機器樣板.value = `vm-${slug}-{n}`
    起始.value = 1
    補零.value = 2
    位址樣板.value = '10.0.0.{ip}:0'
    位址起始.value = 11
    機器種類.value = 'virtual-machine'
    接點.value = 接點們.value[0]?.id ?? null
    放在.value = null
    // 開起來就先算一次。下面那個 watch 只看得到欄位「變動」，
    // 而剛填好預設值的這一刻它還沒被建立。
    void 重算()
  },
  { immediate: true },
)

/** 每次改動都重算。判斷全在 Rust——撞名、接點對不對，都不是這裡說了算。 */
async function 重算() {
  const 要建的 = store.新增機器中
  const def = 接點.value
  if (!要建的 || !def) return

  const spec = 規格(def)
  const 回應 = await commands.previewBatch(要建的.environment, spec)
  if (回應.status === 'ok') {
    plan.value = 回應.data
    擋下來的.value = null
  } else {
    plan.value = null
    擋下來的.value = (回應.error as { message?: string })?.message ?? String(回應.error)
  }
}

function 規格(def: Id): BatchSpec {
  const ep = 接點們.value.find((e) => e.id === def)
  return {
    count: 數量.value,
    nameTemplate: 名稱樣板.value,
    nodeTemplate: 機器樣板.value,
    start: 起始.value,
    pad: 補零.value,
    addressTemplate: 位址樣板.value,
    ipStart: 位址起始.value,
    nodeKind: 機器種類.value,
    container: store.新增機器中!.container,
    endpoint: { def, slug: ep?.slug ?? 'port', protocol: ep?.protocol ?? 'tcp' },
  }
}

watch(
  [數量, 名稱樣板, 機器樣板, 起始, 補零, 位址樣板, 位址起始, 機器種類, 接點],
  () => void 重算(),
)

async function 建立() {
  const 要建的 = store.新增機器中
  if (!要建的 || !plan.value) return
  const nodes = plan.value.nodes
  store.新增機器中 = null
  await store.套用編輯({
    addInstances: { environment: 要建的.environment, within: 放在.value, nodes },
  })
}
</script>

<template>
  <div v-if="store.新增機器中" class="scrim" @click.self="store.新增機器中 = null">
    <section class="box" role="dialog" aria-modal="true">
      <h2>批次建立機器</h2>
      <p class="muted sub">
        <span class="mono">{{ store.新增機器中.label }}</span>
        ・{{ store.環境名(store.新增機器中.environment) }}
      </p>

      <div class="grid">
        <label>數量<input v-model.number="數量" type="number" min="1"></label>
        <label>起始序號<input v-model.number="起始" type="number" min="0"></label>
        <label>補零位數<input v-model.number="補零" type="number" min="0"></label>
        <label class="wide">服務名稱樣板<input v-model="名稱樣板" type="text" class="mono"></label>
        <label class="wide">機器名稱樣板<input v-model="機器樣板" type="text" class="mono"></label>
        <label class="wide">位址樣板<input v-model="位址樣板" type="text" class="mono"></label>
        <label>位址起始<input v-model.number="位址起始" type="number" min="0"></label>
        <label>機器種類
          <select v-model="機器種類">
            <option value="virtual-machine">虛擬機</option>
            <option value="physical">實體機</option>
            <option value="linux-container">Linux 容器</option>
          </select>
        </label>
        <label>接點
          <select v-model="接點">
            <option v-for="e in 接點們" :key="e.id" :value="e.id">{{ e.slug }}</option>
          </select>
        </label>
        <label class="wide">放在哪個節點底下
          <select v-model="放在">
            <option :value="null">（環境最上層）</option>
            <option v-for="n in 可放的節點" :key="n.id" :value="n.id">{{ n.label }}</option>
          </select>
        </label>
      </div>

      <p class="muted tip">
        <span class="mono">{{ '{n}' }}</span> 是序號、<span class="mono">{{ '{ip}' }}</span> 是位址序號。
        兩個分開數，因為 <span class="mono">redis-01</span> 常常對到
        <span class="mono">10.0.1.11</span>。
      </p>

      <!-- 一次建六台是會後悔的操作，按下去之前就要看到那六行。 -->
      <p v-if="擋下來的" class="blocked">{{ 擋下來的 }}</p>
      <ul v-else-if="plan" class="preview">
        <li v-for="(line, i) in plan.preview" :key="i" class="mono">{{ line }}</li>
      </ul>

      <footer>
        <span class="muted hint">建錯了可以按 ⌘Z 整批復原</span>
        <span class="grow" />
        <button @click="store.新增機器中 = null">取消</button>
        <button class="primary" :disabled="!plan" @click="建立()">
          建立 {{ plan?.nodes.length ?? 0 }} 台
        </button>
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
  width: min(680px, 94vw);
  max-height: 86vh;
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

.grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; }
.grid label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
  color: var(--ink-2);
  min-width: 0;
}
.grid .wide { grid-column: span 3; }
.grid input, .grid select { min-width: 0; }

.tip { font-size: 11.5px; line-height: 1.6; }

.preview {
  margin: 0;
  padding: 9px 12px;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 3px;
  max-height: 30vh;
  overflow: auto;
  border: 1px solid var(--rule);
  border-radius: 5px;
  background: var(--surface-2);
  font-size: 12px;
}

.blocked {
  padding: 9px 12px;
  border: 1px solid color-mix(in srgb, var(--broken) 40%, transparent);
  border-radius: 5px;
  background: color-mix(in srgb, var(--broken) 8%, transparent);
  color: var(--broken);
  font-size: 12.5px;
}

footer { display: flex; align-items: center; gap: 8px; margin-top: 4px; }
.grow { flex: 1; }
.hint { font-size: 11.5px; }
</style>
