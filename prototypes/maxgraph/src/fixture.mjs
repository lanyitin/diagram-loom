/**
 * 素材：跟 `docs/nested-layout.md` 那一輪**完全一樣**的圖。
 *
 * 一樣才比得出來。換一份素材的話，「elkjs 排得比 draw.io 好還是壞」
 * 這個問題就沒有答案了——差異可能只是素材不同。
 *
 * ```text
 * 台北機房 ─┬─ vm-01 ── order-api
 *           ├─ vm-02 ── redis-01
 *           └─ vm-03 ── redis-02
 * 備援機房 ─── vm-04 ── redis-03
 * F5 VIP（不在任何機房內）
 *
 * order-api → F5 → redis-01 / redis-02 / redis-03
 * ```
 */

/** 服務實體的大小。跟主程式 `toXml` 畫的一樣（180×60 那個是含副標的）。 */
const INSTANCE = { width: 160, height: 60 }
/** 容器的原始尺寸。**故意給得太大**——要看排版會不會把它縮到剛好包住子節點。 */
const CONTAINER = { width: 700, height: 200 }

function instance(id, slug) {
  return { id, loomId: id, label: slug, ...INSTANCE, children: [] }
}

function machine(id, slug, instances) {
  return { id, loomId: id, label: slug, ...CONTAINER, children: instances }
}

/** 這一份是「模型」，還不是排版結果。座標由 ELK 填。 */
export function fixture() {
  return {
    nodes: [
      machine('site-tpe', '台北機房', [
        machine('vm-01', 'vm-01', [instance('order-api', 'order-api')]),
        machine('vm-02', 'vm-02', [instance('redis-01', 'redis-01')]),
        machine('vm-03', 'vm-03', [instance('redis-02', 'redis-02')]),
      ]),
      machine('site-bak', '備援機房', [
        machine('vm-04', 'vm-04', [instance('redis-03', 'redis-03')]),
      ]),
      { id: 'f5', loomId: 'f5', label: 'F5 VIP', width: 160, height: 60, children: [] },
    ],
    edges: [
      { id: 'e1', from: 'order-api', to: 'f5' },
      { id: 'e2', from: 'f5', to: 'redis-01' },
      { id: 'e3', from: 'f5', to: 'redis-02' },
      { id: 'e4', from: 'f5', to: 'redis-03' },
    ],
  }
}

/**
 * 大一點的圖，用來量時間。
 *
 * 形狀照真實的樣子長：一個站點、`machines` 台機器、每台幾個服務，
 * 加上一個 hub 把所有服務串起來（真實的圖就是這樣，不是隨機連線）。
 */
export function crowd(machines, perMachine) {
  const nodes = []
  const edges = []
  for (let m = 0; m < machines; m += 1) {
    const children = []
    for (let i = 0; i < perMachine; i += 1) {
      const id = `svc-${m}-${i}`
      children.push(instance(id, id))
      edges.push({ id: `e-${id}`, from: id, to: 'hub' })
    }
    nodes.push(machine(`vm-${m}`, `vm-${m}`, children))
  }
  return {
    nodes: [
      { id: 'site', loomId: 'site', label: '機房', ...CONTAINER, children: nodes },
      { id: 'hub', loomId: 'hub', label: 'F5', width: 160, height: 60, children: [] },
    ],
    edges,
  }
}

/** 圖上總共幾個節點（含容器）。 */
export function countNodes(nodes) {
  return nodes.reduce((n, node) => n + 1 + countNodes(node.children ?? []), 0)
}
