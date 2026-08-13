<script setup lang="ts">
/**
 * 表格的欄名列：排序、拖寬、挑要顯示哪些欄。
 *
 * # 為什麼是一個共用元件
 *
 * 資源表與連線表本來各寫了一份排序。那時候只是重複，還不痛；
 * 加上拖寬與欄位選單之後，重複的部分會變成三倍，而**兩份實作一定會
 * 各自演化**——最後使用者在兩張表上得到兩種不一樣的操作感。
 *
 * # 寬度為什麼要先量一次
 *
 * 表格預設是 `table-layout: auto`，瀏覽器照內容自己算欄寬，算得比我們好。
 * 但那個模式下**寬度是拖不動的**：內容加上 `white-space: nowrap` 就是
 * 一道下限，你把欄拉窄它會自己彈回去。
 *
 * 要拖得動就得換成 `table-layout: fixed`，而 fixed 需要每一欄都有明確
 * 寬度，否則它會把空間平均分掉——一張本來排得好好的表會瞬間變形。
 *
 * 所以順序是：**先讓瀏覽器 auto 排一次 → 量下來 → 才切成 fixed。**
 * 量到的值只活在這個元件裡，不寫進 localStorage（見 `lib/columns.ts`）。
 */
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { MIN_WIDTH, type ColumnPrefs } from '../lib/columns'
import { nextSort, type Sort } from '../lib/rows'

/*
 * ⚠️ `menu` 一定要走 `withDefaults`。
 *
 * Vue 對布林 prop 有特別待遇：**沒傳的時候是 `false`，不是 `undefined`**
 * （那是為了讓 `<Foo disabled>` 這種寫法成立）。所以 `menu?: boolean` 加上
 * 「`!== false` 就顯示」會變成兩張沒傳這個 prop 的表通通不顯示——
 * 而它不會報錯，只是那顆鈕安靜地消失。
 */
const props = withDefaults(defineProps<{
  /** 全部欄名，資料的順序。 */
  columns: string[]
  /** 目前要畫的欄名。由 `useColumns().visible()` 算出來。 */
  visible: string[]
  prefs: ColumnPrefs
  sort: Sort | null
  /** 換一張表就重量一次。通常傳 `Table.kind`。 */
  tableKey: string
  /** 靠右對齊的欄（數字）。 */
  right?: string[]
  /**
   * 要不要放「欄位」那顆鈕。預設要。
   *
   * 覆蓋矩陣傳 `false`：它的欄就是環境，而工具列上的環境勾選已經是那個開關了。
   * 同一件事有兩個入口的話，使用者兩個都不會信。
   */
  menu?: boolean
}>(), { menu: true, right: () => [] })

const emit = defineEmits<{
  'update:sort': [Sort | null]
  toggle: [string]
  resize: [column: string, px: number]
  autofit: [string]
}>()

const head = ref<HTMLTableSectionElement | null>(null)

/** 量出來的寬度。不留下來——資料變了就該重量。 */
const measured = ref<Record<string, number>>({})

/** 量完了沒。量完才切 `fixed`，切早了表會變形。 */
const frozen = computed(() => props.visible.every((c) => width(c) !== undefined))

function width(column: string): number | undefined {
  return props.prefs.widths[column] ?? measured.value[column]
}

/**
 * 照 auto 排出來的結果量一次。
 *
 * 只量還沒有寬度的欄。使用者拖過的欄有自己的值，不能被量測蓋掉。
 */
async function measure() {
  await nextTick()
  const row = head.value?.querySelector('tr')
  if (!row) return

  const next = { ...measured.value }
  for (const th of Array.from(row.querySelectorAll<HTMLTableCellElement>('th[data-column]'))) {
    const column = th.dataset.column!
    if (props.prefs.widths[column] !== undefined) continue
    const w = th.getBoundingClientRect().width
    if (w > 0) next[column] = Math.round(w)
  }
  measured.value = next
}

// 換一張表就把量測值丟掉重來：欄位不一樣，舊的值沒有意義。
watch(() => props.tableKey, () => { measured.value = {} }, { immediate: true })
// 欄位增減（挑了欄、加了環境）之後也要重量，不然新欄沒有寬度、表會變形。
watch(() => [props.tableKey, props.visible.join('\u0000')], () => void measure(), {
  immediate: true,
})

/**
 * 把正在排序的那一欄藏起來時，排序也一起收掉。
 *
 * 不收的話列還是照那一欄排，但**畫面上沒有任何東西能解釋這個順序**——
 * 沒有欄名、沒有箭頭，看起來就是列莫名其妙亂掉了。
 * 「看得到的東西要能解釋看得到的結果」比「保留使用者的選擇」重要。
 */
watch(
  () => props.visible.join('\u0000'),
  () => {
    if (props.sort && !props.visible.includes(props.sort.column)) emit('update:sort', null)
  },
  { flush: 'sync' },
)

function styleFor(column: string) {
  const w = width(column)
  return w === undefined ? undefined : { width: `${w}px` }
}

function clickHeader(column: string) {
  emit('update:sort', nextSort(props.sort, column))
}

function ariaSort(column: string): 'ascending' | 'descending' | 'none' {
  if (props.sort?.column !== column) return 'none'
  return props.sort.direction === 'asc' ? 'ascending' : 'descending'
}

/* ── 拖曳 ──────────────────────────────────────────────
 *
 * 用 `mousemove` 而不是 `pointerdown` + setPointerCapture：這裡只要
 * 滑鼠，而 capture 在 iframe 旁邊的行為比較難預測。
 */

let dragging: { column: string; startX: number; startWidth: number } | null = null

function startResize(column: string, event: MouseEvent) {
  dragging = {
    column,
    startX: event.clientX,
    startWidth: width(column) ?? MIN_WIDTH,
  }
  window.addEventListener('mousemove', onDrag)
  window.addEventListener('mouseup', endResize)
}

function onDrag(event: MouseEvent) {
  if (!dragging) return
  emit('resize', dragging.column, dragging.startWidth + (event.clientX - dragging.startX))
}

function endResize() {
  dragging = null
  window.removeEventListener('mousemove', onDrag)
  window.removeEventListener('mouseup', endResize)
}

onBeforeUnmount(endResize)

/* ── 欄位選單 ───────────────────────────────────────── */

const menuOpen = ref(false)

function closeMenu() {
  menuOpen.value = false
}

function toggleMenu() {
  menuOpen.value = !menuOpen.value
  if (menuOpen.value) window.addEventListener('click', closeMenu, { once: true })
}

onBeforeUnmount(() => window.removeEventListener('click', closeMenu))

defineExpose({ frozen })
</script>

<template>
  <thead ref="head" :class="{ frozen }">
    <tr @keydown.esc="menuOpen = false">
      <!-- 嚴重度圓點、備援標記這類不是「欄位」的欄，由呼叫端自己放。 -->
      <slot name="lead" />

      <th
        v-for="c in visible" :key="c"
        :data-column="c"
        class="sortable"
        :class="{ right: right.includes(c) }"
        :style="styleFor(c)"
        :aria-sort="ariaSort(c)"
        @click="clickHeader(c)"
      >
        {{ c }}
        <!--
          箭頭用 CSS 畫，不放進標題的文字裡——放進去的話欄名會變成
          「名稱 ▴」，螢幕閱讀器會照唸，而排序狀態 `aria-sort` 已經
          講過一次了。透明的那個一直佔著位置，不然排序時整排標題會左右跳。
        -->
        <span class="arrow" aria-hidden="true" />
        <!-- 把手要吃掉點擊，不然拖完會順便觸發排序。 -->
        <span
          class="grip"
          title="拖曳調整欄寬 · 雙擊還給內容"
          @mousedown.stop.prevent="startResize(c, $event)"
          @click.stop
          @dblclick.stop="emit('autofit', c)"
        />
      </th>

      <!-- 最後一欄同時是動作欄的欄名。多開一欄只為了放這顆鈕的話，
           每一列都要陪著空一格。 -->
      <th v-if="menu" class="act">
        <button
          class="cols"
          :aria-expanded="menuOpen"
          title="挑要顯示哪些欄"
          @click.stop="toggleMenu()"
        >欄位</button>
        <div v-if="menuOpen" class="pop" @click.stop>
          <button
            v-for="(c, i) in columns" :key="c"
            :disabled="i === 0"
            @click="emit('toggle', c)"
          >
            <span class="box" :class="{ on: !prefs.hidden.includes(c) }" />
            <span class="name">{{ c }}</span>
            <!-- 第一欄是這一列的名字。關掉之後就不知道在看誰了。 -->
            <span v-if="i === 0" class="lock">固定</span>
          </button>
        </div>
      </th>
    </tr>
  </thead>
</template>

<style scoped>
th {
  position: sticky;
  top: 0;
  z-index: 2;
  text-align: left;
  padding: 0 12px;
  height: var(--row);
  background: var(--surface-2);
  border-bottom: 1px solid var(--rule);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: .06em;
  color: var(--ink-3);
  white-space: nowrap;
}
th.right { text-align: right; }

th.sortable { cursor: pointer; user-select: none; }
th.sortable:hover { color: var(--ink); }

.arrow { margin-left: 4px; font-size: 10px; opacity: 0; }
.arrow::after { content: '▴'; }
/* 平常看不出哪裡可以排序，所以滑過去先淡淡地講一聲。 */
th.sortable:hover .arrow { opacity: .35; }
th[aria-sort='ascending'] .arrow { opacity: .8; }
th[aria-sort='descending'] .arrow { opacity: .8; }
th[aria-sort='descending'] .arrow::after { content: '▾'; }

/* 拖曳把手。平常是一條跟分隔線一樣安靜的細線，滑過去才變成強調色。 */
.grip {
  position: absolute;
  top: 7px;
  bottom: 7px;
  right: -3px;
  width: 6px;
  cursor: col-resize;
  z-index: 3;
}
.grip::after {
  content: '';
  position: absolute;
  inset: 0 2px;
  border-radius: 2px;
  background: var(--rule);
}
.grip:hover::after { inset: 0 1px; background: var(--warp); }

.act {
  position: relative;
  width: 66px;
  padding-left: 0;
  padding-right: 8px;
  text-align: right;
}
.cols {
  padding: 1px 7px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: .04em;
  color: var(--ink-3);
  border-color: transparent;
  background: transparent;
}
.cols:hover { color: var(--ink); border-color: var(--rule); background: var(--surface); }

.pop {
  position: absolute;
  top: calc(100% + 2px);
  right: 6px;
  z-index: 20;
  min-width: 176px;
  padding: 5px 0;
  border: 1px solid var(--rule);
  border-radius: 7px;
  background: var(--raise);
  box-shadow: 0 8px 24px var(--shadow);
  /* 欄名列是小字、加了字距、灰色的——選單裡的字不該跟著。 */
  font-size: 12.5px;
  font-weight: 400;
  letter-spacing: 0;
  color: var(--ink-2);
  text-align: left;
}
.pop button {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 4px 12px;
  border: 0;
  border-radius: 0;
  background: transparent;
  font: inherit;
  color: inherit;
  text-align: left;
}
.pop button:hover:not(:disabled) { background: var(--surface-2); }
.pop .name { overflow: hidden; text-overflow: ellipsis; }
.pop .lock { margin-left: auto; font-family: var(--mono); font-size: 10.5px; color: var(--ink-4); }

/* 勾與不勾要看得出形狀不一樣，不能只差在顏色。 */
.box {
  position: relative;
  width: 12px;
  height: 12px;
  flex: none;
  border: 1px solid var(--ink-4);
  border-radius: 3px;
  background: var(--surface);
}
.box.on { background: var(--warp); border-color: var(--warp); }
.box.on::after {
  content: '';
  position: absolute;
  left: 3px;
  top: 1px;
  width: 4px;
  height: 7px;
  border: solid var(--on-accent);
  border-width: 0 1.6px 1.6px 0;
  transform: rotate(42deg);
}
</style>
