<script setup lang="ts">
/**
 * 形狀庫：使用者自己拖出來的東西。
 *
 * # ⚠️ 拖出來的形狀模型不認得——這件事不藏
 *
 * 圖是從模型產生的，所以本來的判斷是「不該讓使用者拖形狀」。但使用者要能
 * 自由畫圖，所以做了，而顧慮沒有消失，只是換了答案：
 *
 * > **拖出來的形狀不帶 `loomId`**，所以它會自動出現在「標註」那份待辦清單裡，
 * > 並且被算進「還剩幾個沒指定」。
 *
 * # 預覽是真的畫出來的
 *
 * 每一格是一個很小的 `Graph`，把那個形狀真的畫一次。用圖檔或手寫 SVG 比較省，
 * 但那樣**預覽會跟實際畫出來的不一樣**——而形狀庫的唯一功能就是
 * 「讓你先看到它長什麼樣」。draw.io 的 stencil 更是只有它自己畫得出來。
 */
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { Graph } from '@maxgraph/core'

import { parseStyle } from '../lib/graph/mxml'
import { BASIC_SHAPES, MODEL_SHAPES, NETWORK_SHAPES, type Palette, registerStencils } from '../lib/graph/shapes'
import stencilXml from '../assets/stencils/networks.xml?raw'

const emit = defineEmits<{
  /** 使用者挑了一個形狀。放哪裡由畫布決定。 */
  (e: 'pick', shape: Palette): void
  (e: 'close'): void
}>()

/** 註冊 stencil。查不到的話 maxGraph 會**默默退回矩形**，所以要說得出實話。 */
const loaded = ref(0)
const previews = ref<HTMLElement[]>([])
const graphs: Graph[] = []

const groups = [
  { title: '模型', shapes: MODEL_SHAPES },
  { title: '基本', shapes: BASIC_SHAPES },
  { title: '網路（draw.io 形狀庫）', shapes: NETWORK_SHAPES },
]

onMounted(() => {
  loaded.value = registerStencils(stencilXml)

  // 預覽晚一步畫，先把版面長出來——二十幾個小 Graph 一起建會卡一下。
  queueMicrotask(() => {
    const all = groups.flatMap((g) => g.shapes)
    previews.value.forEach((host, i) => {
      const shape = all[i]
      if (!host || !shape) return
      const graph = new Graph(host)
      graph.setEnabled(false)
      const scale = Math.min(44 / shape.width, 32 / shape.height)
      graph.batchUpdate(() => {
        graph.insertVertex({
          parent: graph.getDefaultParent(),
          value: '',
          position: [2, 2],
          size: [Math.max(4, shape.width * scale), Math.max(4, shape.height * scale)],
          style: parseStyle(shape.style),
        })
      })
      graphs.push(graph)
    })
  })
})

onBeforeUnmount(() => {
  for (const graph of graphs) graph.destroy()
})
</script>

<template>
  <aside class="picker">
    <div class="top">
      <strong class="title">形狀</strong>
      <button class="x" aria-label="關閉" @click="emit('close')">✕</button>
    </div>

    <template v-for="group in groups" :key="group.title">
      <h2 v-if="group.shapes.length">{{ group.title }}</h2>
      <div v-if="group.shapes.length" class="grid">
        <button
          v-for="shape in group.shapes" :key="shape.label"
          class="tile" :title="shape.label" :data-shape="shape.label"
          @click="emit('pick', shape)"
        >
          <span ref="previews" class="preview" />
          <span class="name">{{ shape.label }}</span>
        </button>
      </div>
    </template>

    <p class="foot muted">
      <!-- 不講的話，使用者會以為拖出來的框就等於模型裡多了一台機器。 -->
      拖出來的形狀<strong>模型還不認得</strong>，會落在「標註」那份清單裡等你指定。
    </p>
  </aside>
</template>

<style scoped>
.picker {
  width: 168px;
  border-right: 1px solid var(--rule);
  overflow-y: auto;
  background: var(--surface);
  display: flex;
  flex-direction: column;
}

.top { display: flex; align-items: center; gap: 8px; padding: 8px 10px 4px; }
.title { flex: 1; font-size: 12.5px; }
.x { background: none; border: 0; padding: 0 2px; color: var(--ink-3); cursor: pointer; font-size: 12px; }

h2 {
  margin: 0; padding: 8px 10px 4px;
  font-size: 10.5px; font-weight: 600; color: var(--ink-3);
  text-transform: uppercase; letter-spacing: 0.05em;
}

.grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: 4px; padding: 0 8px; }

.tile {
  display: flex; flex-direction: column; align-items: center; gap: 2px;
  padding: 4px 2px;
  border: 1px solid transparent; border-radius: 5px;
  background: none; cursor: pointer;
  font: inherit; font-size: 10.5px; color: var(--ink-3);
}
.tile:hover { border-color: var(--warp); color: var(--warp); background: #fff; }
.preview { width: 48px; height: 34px; overflow: hidden; pointer-events: none; }
.name { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.foot {
  margin: 10px 0 0;
  padding: 8px 10px;
  border-top: 1px solid var(--rule);
  font-size: 11px;
  line-height: 1.6;
}
</style>
