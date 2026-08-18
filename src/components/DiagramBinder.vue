<script setup lang="ts">
/**
 * 標註面板：圖上還沒指定代表誰的形狀，做成一張待辦清單。
 *
 * # 為什麼是清單，不是在圖上點
 *
 * 直覺的做法是點一個方框、跳出選單問它是誰。但**嵌入協定沒有選取事件**，
 * 站在 iframe 外面永遠不知道使用者選了哪個形狀（見 `docs/canvas-engine.md`）。
 *
 * 把流程反過來就成立了，而且可能更好：這個工具的命是**怕漏**。點圖一次處理
 * 一個，使用者得自己記得去點每一個框，永遠不知道還剩幾個沒點；
 * 清單天生會說「還剩 12 個」。
 *
 * # 建議只是建議
 *
 * 名字對得上的那幾個會出現一顆「就是它」的按鈕，但**不會自己套用**。
 * 綁定之後對帳就把它當事實，而錯的那一項不會有人再檢查它——
 * 所以每一個指定都要有人按下去。
 */
import { computed } from 'vue'
import Picker from './Picker.vue'
import type { Annotation, ElementKind, Target, UnboundShape } from '../lib/model'

const props = defineProps<{
  /** 圖上還沒指定的形狀。 */
  shapes: UnboundShape[]
  /** Rust 算出來的：可以指給誰、猜得到哪幾個。 */
  annotation: Annotation
  /** 這一輪已經指定好的，留著讓人收得回來。 */
  assigned: { cell: string; label: string; target: string }[]
  /** 現在圖上標示著哪一個。 */
  spot: string | null
}>()

const emit = defineEmits<{
  (e: 'assign', cell: string, target: string): void
  (e: 'unassign', cell: string): void
  (e: 'spot', cell: string | null): void
  (e: 'close'): void
}>()

/**
 * 分組的標題。
 *
 * 型別是 `Record<ElementKind, string>`，不是 `Record<string, string>`：
 * 少一種就編不過。鬆的那種寫法會讓新加的種類**安靜地**以 kebab-case 原文
 * 當標題（`software-system`），而那看起來像資料髒掉，不像少寫了一行。
 */
const KIND: Record<ElementKind, string> = {
  // 環境層（分身）。詳圖畫的就是這些。
  'deployment-node': '機器',
  'container-instance': '服務實體',
  'infrastructure-node': '設備',
  'software-system-instance': '外部系統實體',
  connection: '連線',
  // 邏輯層（母版）。**只有簡圖才會出現。**
  // Context 圖畫人與系統、Container 圖畫服務與契約，而使用者自己畫的圖
  // 常常兩種混在一起。
  person: '人',
  'software-system': '系統',
  container: '服務',
  relationship: '契約',
}

/** 沒有文字的形狀多半是裝飾，但照樣要列——自己決定「這個不用管」就是漏。 */
const nameOf = (shape: UnboundShape) => shape.label || '（沒有文字）'

function options(shape: UnboundShape) {
  // 線只能指給連線、框只能指給元素。混在一起的話，一個方框可以被指成一條
  // 連線，而那在對帳時是個永遠對不上、又看不出怎麼來的東西。
  const targets = shape.edge ? props.annotation.connections : props.annotation.shapes
  return targets.map((t) => ({ value: t.id, label: t.label, group: KIND[t.kind] }))
}

const byId = computed(() => {
  const out = new Map<string, Target>()
  for (const t of [...props.annotation.shapes, ...props.annotation.connections]) out.set(t.id, t)
  return out
})

const guesses = computed(() => new Map(props.annotation.guesses.map((g) => [g.cell, g.target])))

function guessLabel(cell: string): string | null {
  const target = guesses.value.get(cell)
  return target ? (byId.value.get(target)?.label ?? null) : null
}

/** 已經指定走的名字。清單上顯示用，不必再去問 Rust。 */
const assignedLabel = (target: string) => byId.value.get(target)?.label ?? target
</script>

<template>
  <div class="card" role="dialog" aria-label="標註">
    <div class="top">
      <strong class="title">標註</strong>
      <span class="left">還剩 {{ shapes.length }} 個</span>
      <button class="x" aria-label="關閉" @click="emit('close')">✕</button>
    </div>

    <ul v-if="shapes.length" class="list">
      <li v-for="s in shapes" :key="s.cell">
        <div class="row">
          <button
            class="mark"
            :class="{ on: spot === s.cell }"
            :aria-pressed="spot === s.cell"
            title="在圖上標示這一個"
            @click="emit('spot', spot === s.cell ? null : s.cell)"
          >◎</button>
          <span class="name" :class="{ blank: !s.label }" :title="nameOf(s)">{{ nameOf(s) }}</span>
          <span v-if="s.edge" class="tag">線</span>
        </div>
        <div class="pick">
          <Picker
            :model-value="null"
            :options="options(s)"
            placeholder="指定成…"
            @update:model-value="$event && emit('assign', s.cell, $event)"
          />
          <!-- 猜得到的才有這顆按鈕，而且要有人按。名字對得上不等於就是它。 -->
          <button
            v-if="guessLabel(s.cell)"
            class="guess"
            @click="emit('assign', s.cell, guesses.get(s.cell)!)"
          >就是 {{ guessLabel(s.cell) }}</button>
        </div>
      </li>
    </ul>

    <p v-else class="muted done">
      這張圖上的形狀都指定過了。
    </p>

    <details v-if="assigned.length" class="undo">
      <summary>這一輪指定了 {{ assigned.length }} 個</summary>
      <ul class="list">
        <li v-for="a in assigned" :key="a.cell" class="row">
          <span class="name" :title="a.label">{{ a.label || '（沒有文字）' }}</span>
          <span class="arrow">→</span>
          <span class="name mono">{{ assignedLabel(a.target) }}</span>
          <button class="link" @click="emit('unassign', a.cell)">取消</button>
        </li>
      </ul>
    </details>

    <p class="muted foot">
      <!-- 沒指定的形狀對帳看不見，所以它不會被算成缺漏，也不會有人叫。
           不講的話，使用者會以為「圖上有畫」就等於「這件事管到了」。 -->
      沒有指定的形狀<strong>對帳看不見</strong>——圖上畫了也不算數。
      <br>
      指定寫在這張圖裡，而圖還沒有存檔的功能，所以按「重畫」會全部回到原點。
    </p>
  </div>
</template>

<style scoped>
.card {
  position: absolute;
  top: 6px;
  left: 8px;
  z-index: 5;
  width: 280px;
  max-height: calc(100% - 20px);
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px;
  border: 1px solid var(--rule);
  border-radius: 6px;
  background: var(--surface);
  box-shadow: 0 6px 20px color-mix(in srgb, #000 22%, transparent);
  font-size: 12.5px;
}

.top { display: flex; align-items: center; gap: 8px; }
.title { flex: 1; font-size: 12.5px; }
.left { font-size: 11.5px; color: var(--ink-3); font-variant-numeric: tabular-nums; }
.x { background: none; border: 0; padding: 0 2px; color: var(--ink-3); cursor: pointer; font-size: 12px; }

.list { flex: 1; min-height: 0; overflow-y: auto; margin: 0; padding: 0; list-style: none; }
.list > li + li { margin-top: 8px; padding-top: 8px; border-top: 1px solid var(--rule); }

.row { display: flex; align-items: center; gap: 6px; }
.name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.name.blank { color: var(--ink-3); font-style: italic; }
.mono { font-family: var(--mono, monospace); }
.arrow { color: var(--ink-3); }

.mark {
  flex: none;
  padding: 0 4px;
  border: 0;
  background: none;
  color: var(--ink-3);
  cursor: pointer;
}
.mark.on { color: var(--warp); }

.tag { flex: none; font-size: 11px; color: var(--ink-3); }

.pick { display: flex; align-items: center; gap: 6px; margin-top: 4px; }
.pick > :first-child { flex: 1; min-width: 0; }

.guess {
  flex: none;
  max-width: 45%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11.5px;
}

.link { background: none; border: 0; padding: 0; color: var(--warp); cursor: pointer; font-size: 11.5px; }

.undo { font-size: 11.5px; }
.undo summary { cursor: pointer; color: var(--ink-3); }
.undo .list { max-height: 140px; margin-top: 6px; }

.done { padding: 8px 0; }

.foot {
  padding-top: 8px;
  border-top: 1px solid var(--rule);
  font-size: 11.5px;
  line-height: 1.6;
}
</style>
