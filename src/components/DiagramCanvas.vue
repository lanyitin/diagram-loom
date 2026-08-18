<script setup lang="ts">
/**
 * 畫布：maxGraph 的容器。**這支取代了以前那個 iframe。**
 *
 * # 為什麼它這麼薄
 *
 * 判斷都不在這裡。排版在 `graph/layout.ts`、建 cell 在 `graph/render.ts`、
 * 綁定與螢光筆在 `graph/binding.ts` 與 `graph/highlight.ts`——那些都不需要
 * 畫面，所以都在秒級測試裡守著。
 *
 * 這個元件只負責三件畫面才做得到的事：**把 maxGraph 掛上 DOM**、
 * **把它的事件轉成 Vue 的事件**、**把操作介面交出去**（`defineExpose`）。
 *
 * # 跟 iframe 那版比，消失的東西
 *
 * `postMessage` 的協定、`autosave` 事件、等排版跑完才能縮放的計時器、
 * 逾時處理、`zoom` 穿透 iframe 的抵銷（`--unzoom`）——**全部不見了**，
 * 因為那些都是「隔著一層 iframe」才有的問題。
 */
import { onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue'
import { type Cell, type FitPlugin, Graph, InternalEvent, type UndoManager } from '@maxgraph/core'

import type { Shape } from '../lib/diagram'
import type { Link } from '../lib/model'
import { centerOn, connecting, foldIcons, panAndZoom, paintGrid, undoStack, useElementValues } from '../lib/graph/canvas'
import { layout } from '../lib/graph/layout'
import { readXml, writeXml } from '../lib/graph/mxml'
import { render } from '../lib/graph/render'

const props = defineProps<{
  /** 從模型產出來的形狀。給 `xml` 的時候不看這個。 */
  shapes: Shape[]
  links: Link[]
  /** 存過的圖。有給就讀它，沒有就從模型產一張。 */
  xml?: string | null
}>()

const emit = defineEmits<{
  /** 圖被改動了（拖曳、改樣式、加東西）。畫面拿它標「未存檔」。 */
  (e: 'change'): void
  /** 選取變了。面板跟著它換內容。 */
  (e: 'select', cells: Cell[]): void
  /** 排版跑完了，附上花的時間。 */
  (e: 'laid', ms: number): void
}>()

/** 自動縮放的上限。一張只有兩個框的圖被放到 400% 只會讓人以為壞了。 */
const MAX_SCALE = 1.2

const host = ref<HTMLDivElement | null>(null)
const graph = shallowRef<Graph | null>(null)
const undo = shallowRef<UndoManager | null>(null)

/** 我們自己灌進去的變更不算「使用者改的」，否則一開圖就標成未存檔。 */
let quiet = 0

/**
 * 重畫了幾次。
 *
 * 兩條路的速度差很多：讀存過的圖是同步的，從模型排版要等 elkjs。所以
 * **後要求的那次可能先畫完**，而先要求的那次晚一步落地就把它蓋掉。
 * 使用者看到的是「我打開自己排過版的圖，出來的卻是自動排的版」，
 * 沒有任何訊息說明發生了什麼。
 */
let draws = 0

let repaintGrid = () => {}

onMounted(() => {
  if (!host.value) return

  const g = new Graph(host.value)
  // ⚠️ 這行要在畫任何東西之前：形狀的值是 XML 元素，少了它標籤會變成 `object`。
  useElementValues(g)
  foldIcons(g)

  g.setCellsMovable(true)
  g.setCellsResizable(true)
  g.setCellsEditable(true)
  // 放得進容器。真實的架構圖是三到四層巢狀的，關掉這條就畫不出來——
  // 而且它壞掉的樣子是「疊在上面但父子關係沒變」，看畫面看不出來。
  g.setDropEnabled(true)
  g.setExtendParents(true)
  g.setExtendParentsOnAdd(true)
  g.setConstrainChildren(false)
  // 拖到一條線上不要把那條線切斷——draw.io 有，但很容易誤觸。
  g.setSplitEnabled(false)

  repaintGrid = paintGrid(g, host.value)
  panAndZoom(g, repaintGrid)
  connecting(g)

  undo.value = undoStack(g)
  graph.value = g

  g.getSelectionModel().addListener(InternalEvent.CHANGE, () => {
    emit('select', g.getSelectionCells())
  })
  g.getDataModel().addListener(InternalEvent.CHANGE, () => {
    if (quiet > 0) quiet -= 1
    else emit('change')
  })

  void draw()
})

onBeforeUnmount(() => {
  graph.value?.destroy()
  graph.value = null
})

/** 重畫。`xml` 有給就讀它，沒有就從模型排一張新的。 */
async function draw() {
  const g = graph.value
  if (!g) return

  const mine = (draws += 1)

  // 灌進去的每一筆都不算使用者改的。給大一點——讀檔會產生好幾筆變更。
  quiet = 3

  if (props.xml) {
    g.batchUpdate(() => readXml(g.getDataModel(), props.xml!))
  } else {
    const laid = await layout(props.shapes, props.links)
    // 排版跑的時候又有人要求重畫，就讓新的那次說了算。
    if (mine !== draws) return
    g.batchUpdate(() => render(g.getDataModel(), props.shapes, props.links, laid))
    emit('laid', laid.ms)
  }

  fit()
  repaintGrid()
}

function fit() {
  const plugin = graph.value?.getPlugin<FitPlugin>('fit')
  // `fitCenter` 只吃 margin；上限縮放要另外壓，否則一張小圖會被放到很誇張。
  plugin?.fitCenter({ margin: 8 })
  const view = graph.value?.getView()
  if (view && view.scale > MAX_SCALE) view.setScale(MAX_SCALE)
  repaintGrid()
}

// 模型、連線或檔案換了就重畫。**存過的圖不會因為模型變動而自動重產**——
// 那會洗掉使用者排好的版面，要不要重畫由他決定（見 `drawio-integration.md`）。
//
// ⚠️ `links` 一定要在裡面。少了它，掛上去那次會用「還沒拿到連線」的狀態畫完
// 就不再動——而零條邊的排版是一整條直的，看起來像排版引擎壞了。
watch(() => [props.shapes, props.links, props.xml], () => void draw(), { deep: false })

/** 灌一次不算使用者改的變更。面板改樣式時包在這裡面。 */
function silently(fn: () => void) {
  quiet += 1
  fn()
}

defineExpose({
  graph,
  undo,
  fit,
  draw,
  silently,
  /** 把某個形狀移到畫面正中間。搜尋用。 */
  center: (cellId: string) => {
    const g = graph.value
    const moved = g ? centerOn(g, cellId) : false
    if (moved) repaintGrid()
    return moved
  },
  /** 存檔用的那一份。**螢光筆要先擦掉**，不然 opacity 會被存進檔案。 */
  toXml: (meta: { id: string; name: string }) => {
    const g = graph.value
    return g ? writeXml(g.getDataModel(), meta) : ''
  },
})
</script>

<template>
  <div ref="host" class="canvas" />
</template>

<style scoped>
/*
 * 格線是**畫布自己的 background**，不是一層蓋上去的 div。
 *
 * ⚠️ 用 div 會蓋住圖：那個 div 是 absolute、maxGraph 的 <svg> 是 static，
 * 而定位過的元素會畫在未定位的內容之上——跟 DOM 順序無關。
 * 元素的 background 則永遠畫在自己的子孫底下。
 */
.canvas {
  flex: 1;
  min-height: 0;
  position: relative;
  overflow: hidden;
  background-color: var(--surface);
  background-image:
    linear-gradient(to right, color-mix(in srgb, var(--rule) 55%, transparent) 1px, transparent 1px),
    linear-gradient(to bottom, color-mix(in srgb, var(--rule) 55%, transparent) 1px, transparent 1px);
}
</style>
