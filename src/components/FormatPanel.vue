<script setup lang="ts">
/**
 * 格式面板：draw.io 右邊那一排。
 *
 * 它服務的是四個目標裡的**容易閱讀**——一張圖要進文件與簡報，
 * 顏色、字級、對齊就是可讀性本身。
 *
 * # 三條設計決定
 *
 * 1. **不分頁，一次攤開。** draw.io 分 Style / Text / Arrange 三頁，因為它有
 *    四十幾個控制項。我們的少，分頁只是多一次點擊。
 * 2. **選了什麼就出現什麼**：選框給形狀／文字／幾何，選線給線，多選才給排列，
 *    什麼都沒選就整片收起來只留一句話。空面板跟「壞掉了」長得一樣。
 * 3. **控制項不自己記狀態。** 每次選取改變或圖被改動（復原、對齊、程式產生）
 *    都整批重新灌值。面板停在舊值的話，使用者會相信面板寫的那個。
 */
import { computed } from 'vue'
import type { Cell, Graph } from '@maxgraph/core'

const props = defineProps<{
  graph: Graph | null
  /** 目前選取。由外面傳進來，面板自己不記。 */
  selection: Cell[]
}>()

const emit = defineEmits<{ (e: 'change'): void }>()

/** 粗體 1、斜體 2、底線 4。`FONT_STYLE_MASK` 有定義但 `index.js` 沒有 re-export。 */
const FONT = { bold: 1, italic: 2, underline: 4 } as const

const FONTS = ['Helvetica', 'Arial', 'Georgia', 'Courier New', 'Noto Sans TC']
const EDGE_STYLES = [
  ['orthogonalEdgeStyle', '直角'],
  ['', '直線'],
  ['elbowEdgeStyle', '折線'],
] as const
const ARROWS = [
  ['classic', '實心箭頭'], ['open', '開放箭頭'], ['block', '方塊'],
  ['diamond', '菱形'], ['oval', '圓'], ['none', '無'],
] as const

const vertices = computed(() => props.selection.filter((c) => c.isVertex()))
const edges = computed(() => props.selection.filter((c) => c.isEdge()))
const one = computed(() => (vertices.value.length === 1 ? vertices.value[0]! : null))

/** 面板顯示的值一律**回頭問圖**，不留自己的副本。 */
const style = computed<Record<string, unknown>>(() => {
  const [cell] = props.selection
  return cell ? { ...(cell.getStyle() as Record<string, unknown>) } : {}
})
const geometry = computed(() => one.value?.getGeometry() ?? null)
const fontStyle = computed(() => Number(style.value.fontStyle ?? 0))

/** 改樣式的共同入口。**一定要包成一筆**，否則復原要按很多次。 */
function set(key: string, value: unknown) {
  const graph = props.graph
  if (!graph) return
  graph.batchUpdate(() => graph.setCellStyles(key as never, value as never, props.selection))
  emit('change')
}

function toggleFont(bit: number, on: boolean) {
  const graph = props.graph
  if (!graph) return
  graph.batchUpdate(() => graph.setCellStyleFlags('fontStyle' as never, bit, on, props.selection))
  emit('change')
}

function resize(axis: 'x' | 'y' | 'width' | 'height', value: number) {
  const graph = props.graph
  const cell = one.value
  if (!graph || !cell) return
  const geo = cell.getGeometry()?.clone()
  if (!geo) return
  geo[axis] = value
  graph.batchUpdate(() => graph.model.setGeometry(cell, geo))
  emit('change')
}

function act(fn: () => void) {
  const graph = props.graph
  if (!graph) return
  graph.batchUpdate(fn)
  emit('change')
}

/** 等距排開。maxGraph 有 `alignCells`，**但沒有分佈**，所以自己算。 */
function distribute(axis: 'x' | 'y', size: 'width' | 'height') {
  const graph = props.graph
  if (!graph || props.selection.length < 3) return
  const boxes = props.selection
    .map((cell) => ({ cell, geo: cell.getGeometry()?.clone() }))
    .filter((b): b is { cell: Cell; geo: NonNullable<typeof b.geo> } => Boolean(b.geo))
    .sort((a, b) => a.geo[axis] - b.geo[axis])
  if (boxes.length < 3) return

  const first = boxes[0]!.geo[axis]
  const last = boxes[boxes.length - 1]!
  const span = last.geo[axis] + last.geo[size] - first
  const used = boxes.reduce((n, b) => n + b.geo[size], 0)
  const gap = (span - used) / (boxes.length - 1)

  act(() => {
    let at = first
    for (const box of boxes) {
      box.geo[axis] = Math.round(at)
      at += box.geo[size] + gap
      graph.model.setGeometry(box.cell, box.geo)
    }
  })
}

/** 清掉轉彎點——線彈回自動走法。 */
function straighten() {
  const graph = props.graph
  if (!graph) return
  act(() => {
    for (const cell of edges.value) {
      const geo = cell.getGeometry()?.clone()
      if (!geo) continue
      geo.points = []
      graph.model.setGeometry(cell, geo)
    }
  })
}

/** `<input type=color>` 只吃 #rrggbb。給它別的會被瀏覽器換成黑色。 */
const hex = (value: unknown, fallback: string) =>
  typeof value === 'string' && /^#[0-9a-f]{6}$/i.test(value) ? value : fallback

const ALIGN = [
  ['left', '⇤', '靠左對齊'], ['center', '⇔', '水平置中'], ['right', '⇥', '靠右對齊'],
  ['top', '⇧', '靠上對齊'], ['middle', '⇕', '垂直置中'], ['bottom', '⇩', '靠下對齊'],
] as const
</script>

<template>
  <aside class="format">
    <p v-if="!selection.length" class="empty muted">選一個形狀或一條線。</p>

    <template v-else>
      <section v-if="vertices.length" class="group">
        <h2>形狀</h2>
        <label class="row">
          <span class="label">填色</span>
          <input
            type="color" :value="hex(style.fillColor, '#ffffff')"
            @input="set('fillColor', ($event.target as HTMLInputElement).value)"
          >
          <!-- `fillColor=none` 跟「黑色」是兩件事，而 <input type=color> 表達不了
               「沒有顏色」。我們的容器框正是 none，少了這個就改不回透明。 -->
          <label class="none" title="無">
            <input
              type="checkbox" :checked="style.fillColor === 'none'"
              @change="set('fillColor', ($event.target as HTMLInputElement).checked ? 'none' : '#ffffff')"
            > 無
          </label>
        </label>
        <label class="row">
          <span class="label">框線</span>
          <input
            type="color" :value="hex(style.strokeColor, '#4d6a9a')"
            @input="set('strokeColor', ($event.target as HTMLInputElement).value)"
          >
          <label class="none" title="無">
            <input
              type="checkbox" :checked="style.strokeColor === 'none'"
              @change="set('strokeColor', ($event.target as HTMLInputElement).checked ? 'none' : '#4d6a9a')"
            > 無
          </label>
          <input
            class="num" type="number" min="0" max="24" step="0.5" :value="style.strokeWidth ?? 1"
            @input="set('strokeWidth', Number(($event.target as HTMLInputElement).value))"
          >
        </label>
        <div class="row">
          <span class="label">樣式</span>
          <button class="toggle" :aria-pressed="!!style.dashed" @click="set('dashed', !style.dashed)">虛線</button>
          <button class="toggle" :aria-pressed="!!style.rounded" @click="set('rounded', !style.rounded)">圓角</button>
          <button class="toggle" :aria-pressed="!!style.shadow" @click="set('shadow', !style.shadow)">陰影</button>
        </div>
        <label class="row">
          <span class="label">透明度</span>
          <input
            class="num" type="number" min="0" max="100" step="5" :value="style.opacity ?? 100"
            @input="set('opacity', Number(($event.target as HTMLInputElement).value))"
          >
        </label>
      </section>

      <section class="group">
        <h2>文字</h2>
        <label class="row">
          <span class="label">字型</span>
          <select :value="style.fontFamily ?? 'Helvetica'" @change="set('fontFamily', ($event.target as HTMLSelectElement).value)">
            <option v-for="f in FONTS" :key="f" :value="f">{{ f }}</option>
          </select>
        </label>
        <label class="row">
          <span class="label">大小</span>
          <input
            class="num" type="number" min="6" max="72" :value="style.fontSize ?? 12"
            @input="set('fontSize', Number(($event.target as HTMLInputElement).value))"
          >
          <input
            type="color" :value="hex(style.fontColor, '#2b2b2b')"
            @input="set('fontColor', ($event.target as HTMLInputElement).value)"
          >
        </label>
        <div class="row">
          <span class="label">樣式</span>
          <button class="toggle" :aria-pressed="!!(fontStyle & FONT.bold)" title="粗體" @click="toggleFont(FONT.bold, !(fontStyle & FONT.bold))">B</button>
          <button class="toggle" :aria-pressed="!!(fontStyle & FONT.italic)" title="斜體" @click="toggleFont(FONT.italic, !(fontStyle & FONT.italic))">I</button>
          <button class="toggle" :aria-pressed="!!(fontStyle & FONT.underline)" title="底線" @click="toggleFont(FONT.underline, !(fontStyle & FONT.underline))">U</button>
        </div>
        <label class="row">
          <span class="label">對齊</span>
          <select :value="style.align ?? 'center'" @change="set('align', ($event.target as HTMLSelectElement).value)">
            <option value="left">靠左</option><option value="center">置中</option><option value="right">靠右</option>
          </select>
          <select :value="style.verticalAlign ?? 'middle'" @change="set('verticalAlign', ($event.target as HTMLSelectElement).value)">
            <option value="top">靠上</option><option value="middle">置中</option><option value="bottom">靠下</option>
          </select>
        </label>
      </section>

      <!-- 打字改座標不是可有可無的：拖曳永遠對不齊，而「容易閱讀」有一半就是對齊。 -->
      <section v-if="one && geometry" class="group">
        <h2>位置與大小</h2>
        <label class="row">
          <span class="label">位置</span>
          <input class="num" type="number" :value="Math.round(geometry.x)" @input="resize('x', Number(($event.target as HTMLInputElement).value))">
          <input class="num" type="number" :value="Math.round(geometry.y)" @input="resize('y', Number(($event.target as HTMLInputElement).value))">
        </label>
        <label class="row">
          <span class="label">大小</span>
          <input class="num" type="number" min="1" :value="Math.round(geometry.width)" @input="resize('width', Number(($event.target as HTMLInputElement).value))">
          <input class="num" type="number" min="1" :value="Math.round(geometry.height)" @input="resize('height', Number(($event.target as HTMLInputElement).value))">
        </label>
      </section>

      <section v-if="edges.length" class="group">
        <h2>線</h2>
        <label class="row">
          <span class="label">走法</span>
          <select :value="style.edgeStyle ?? ''" @change="set('edgeStyle', ($event.target as HTMLSelectElement).value || null)">
            <option v-for="[value, label] in EDGE_STYLES" :key="label" :value="value">{{ label }}</option>
          </select>
        </label>
        <label class="row">
          <span class="label">箭頭</span>
          <select :value="style.startArrow ?? 'none'" @change="set('startArrow', ($event.target as HTMLSelectElement).value)">
            <option v-for="[value, label] in ARROWS" :key="`s${value}`" :value="value">{{ label }}</option>
          </select>
          <select :value="style.endArrow ?? 'classic'" @change="set('endArrow', ($event.target as HTMLSelectElement).value)">
            <option v-for="[value, label] in ARROWS" :key="`e${value}`" :value="value">{{ label }}</option>
          </select>
        </label>
        <div class="row">
          <span class="label">轉彎點</span>
          <button @click="straighten()">拉直</button>
        </div>
      </section>

      <section v-if="selection.length > 1" class="group">
        <h2>排列</h2>
        <div class="row">
          <span class="label">對齊</span>
          <div class="actions">
            <button
              v-for="[value, glyph, title] in ALIGN" :key="value"
              :title="title" :data-action="title"
              @click="act(() => graph!.alignCells(value, selection))"
            >{{ glyph }}</button>
          </div>
        </div>
        <div class="row">
          <span class="label">分佈</span>
          <div class="actions">
            <button title="水平等距" @click="distribute('x', 'width')">⋯</button>
            <button title="垂直等距" @click="distribute('y', 'height')">⋮</button>
          </div>
        </div>
        <div class="row">
          <span class="label">層次</span>
          <div class="actions">
            <button @click="act(() => graph!.orderCells(false, selection))">置前</button>
            <button @click="act(() => graph!.orderCells(true, selection))">置後</button>
          </div>
        </div>
      </section>
    </template>
  </aside>
</template>

<style scoped>
.format { width: 244px; border-left: 1px solid var(--rule); overflow-y: auto; background: var(--surface); }
.empty { padding: 14px 12px; margin: 0; font-size: 12px; }

.group { padding: 8px 12px; border-bottom: 1px solid var(--rule); }
h2 {
  margin: 0 0 6px;
  font-size: 10.5px; font-weight: 600; color: var(--ink-3);
  text-transform: uppercase; letter-spacing: 0.05em;
}

.row { display: flex; align-items: center; gap: 5px; margin-bottom: 5px; }
.row:last-child { margin-bottom: 0; }
.label { flex: none; width: 48px; font-size: 12px; color: var(--ink-2); }

input, select, button { font: inherit; font-size: 12px; height: 24px; }
select { flex: 1; min-width: 0; }
.num { width: 52px; text-align: right; }
input[type='color'] { width: 30px; padding: 1px; }
.none { display: inline-flex; align-items: center; gap: 2px; font-size: 11px; color: var(--ink-3); }
.none input { height: auto; }

.toggle { min-width: 26px; cursor: pointer; }
.toggle[aria-pressed='true'] { background: var(--warp); border-color: var(--warp); color: #fff; }

.actions { display: flex; flex-wrap: wrap; gap: 3px; }
.actions button { min-width: 26px; cursor: pointer; }
</style>
