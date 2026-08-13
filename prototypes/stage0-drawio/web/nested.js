// 巢狀排版的實測。
//
// 階段 0 的第一輪素材只有 2 個節點、沒有巢狀，所以「選 ELK 是因為它處理
// compound node」這個理由等於完全沒被驗到。這一輪補上。
//
// 素材刻意做成真實的形狀：
//
//   台北機房 ─┬─ vm-01 ── order-api
//             ├─ vm-02 ── redis-01
//             └─ vm-03 ── redis-02
//   備援機房 ─── vm-04 ── redis-03
//   F5（不在任何機房內）
//
//   order-api → F5 → redis-01 / redis-02 / redis-03   ← 跨容器的邊，最難的部分
//
// 三層巢狀、兄弟容器、跨容器的邊、一個 hub。這是我們真的會畫的東西。
//
// 判斷排版好不好沒辦法全自動，但「壞掉」有客觀定義：
//   1. 子節點跑到父框外面
//   2. 同一層的兄弟互相重疊
//   3. 父子關係被改掉
//   4. loomId 掉了
// 這四項用程式檢查；剩下的美醜靠匯出的 PNG 人眼看。

const NESTED = {
  dc1: 'aaaa0001-0000-0000-0000-000000000000',
  dc2: 'aaaa0002-0000-0000-0000-000000000000',
  vm1: 'bbbb0001-0000-0000-0000-000000000000',
  vm2: 'bbbb0002-0000-0000-0000-000000000000',
  vm3: 'bbbb0003-0000-0000-0000-000000000000',
  vm4: 'bbbb0004-0000-0000-0000-000000000000',
  api: 'cccc0001-0000-0000-0000-000000000000',
  redis1: 'cccc0002-0000-0000-0000-000000000000',
  redis2: 'cccc0003-0000-0000-0000-000000000000',
  redis3: 'cccc0004-0000-0000-0000-000000000000',
  f5: 'dddd0001-0000-0000-0000-000000000000',
  eIn: 'eeee0001-0000-0000-0000-000000000000',
  eOut1: 'eeee0002-0000-0000-0000-000000000000',
  eOut2: 'eeee0003-0000-0000-0000-000000000000',
  eOut3: 'eeee0004-0000-0000-0000-000000000000',
};

const NODE_STYLE = 'rounded=0;whiteSpace=wrap;html=1;container=1;collapsible=0;verticalAlign=top;';
const LEAF_STYLE = 'rounded=1;whiteSpace=wrap;html=1;';
const EDGE_STYLE = 'edgeStyle=orthogonalEdgeStyle;html=1;';

function nestedFixture(opts = {}) {
  const f5Extra = opts.pinF5 ? 'movable=0;' : '';
  const machine = (id, label, loomId, parent, x, y) => `
        <object label="${label}" loomId="${loomId}" loomKind="deploymentNode" id="${id}">
          <mxCell style="${NODE_STYLE}fillColor=#e8e8e8;" vertex="1" parent="${parent}">
            <mxGeometry x="${x}" y="${y}" width="200" height="120" as="geometry"/>
          </mxCell>
        </object>`;

  const service = (id, label, loomId, parent, fill) => `
        <object label="${label}" loomId="${loomId}" loomKind="containerInstance" id="${id}">
          <mxCell style="${LEAF_STYLE}fillColor=${fill};" vertex="1" parent="${parent}">
            <mxGeometry x="20" y="40" width="160" height="60" as="geometry"/>
          </mxCell>
        </object>`;

  const edge = (id, label, loomId, from, to) => `
        <object label="${label}" loomId="${loomId}" loomKind="connection" id="${id}">
          <mxCell style="${EDGE_STYLE}" edge="1" parent="1" source="${from}" target="${to}">
            <mxGeometry relative="1" as="geometry"/>
          </mxCell>
        </object>`;

  return `<mxfile host="loom">
  <diagram id="prod-nested" name="prod">
    <mxGraphModel dx="1200" dy="800" grid="1" gridSize="10" page="1" pageWidth="1169" pageHeight="826">
      <root>
        <mxCell id="0"/>
        <mxCell id="1" parent="0"/>
        <object label="台北機房" loomId="${NESTED.dc1}" loomKind="deploymentNode" id="n-dc1">
          <mxCell style="${NODE_STYLE}fillColor=#f5f5f5;" vertex="1" parent="1">
            <mxGeometry x="40" y="40" width="700" height="200" as="geometry"/>
          </mxCell>
        </object>
        ${machine('n-vm1', 'vm-01', NESTED.vm1, 'n-dc1', 20, 50)}
        ${machine('n-vm2', 'vm-02', NESTED.vm2, 'n-dc1', 250, 50)}
        ${machine('n-vm3', 'vm-03', NESTED.vm3, 'n-dc1', 480, 50)}
        ${service('n-api', 'order-api', NESTED.api, 'n-vm1', '#dae8fc')}
        ${service('n-redis1', 'redis-01', NESTED.redis1, 'n-vm2', '#d5e8d4')}
        ${service('n-redis2', 'redis-02', NESTED.redis2, 'n-vm3', '#d5e8d4')}
        <object label="備援機房" loomId="${NESTED.dc2}" loomKind="deploymentNode" id="n-dc2">
          <mxCell style="${NODE_STYLE}fillColor=#f5f5f5;" vertex="1" parent="1">
            <mxGeometry x="40" y="280" width="240" height="200" as="geometry"/>
          </mxCell>
        </object>
        ${machine('n-vm4', 'vm-04', NESTED.vm4, 'n-dc2', 20, 50)}
        ${service('n-redis3', 'redis-03', NESTED.redis3, 'n-vm4', '#d5e8d4')}
        <object label="F5 VIP" loomId="${NESTED.f5}" loomKind="infrastructureNode" id="n-f5">
          <mxCell style="rhombus;whiteSpace=wrap;html=1;fillColor=#ffe6cc;${f5Extra}" vertex="1" parent="1">
            <mxGeometry x="800" y="120" width="140" height="80" as="geometry"/>
          </mxCell>
        </object>
        ${edge('e-in', '送進 VIP', NESTED.eIn, 'n-api', 'n-f5')}
        ${edge('e-out1', '分流', NESTED.eOut1, 'n-f5', 'n-redis1')}
        ${edge('e-out2', '分流', NESTED.eOut2, 'n-f5', 'n-redis2')}
        ${edge('e-out3', '分流', NESTED.eOut3, 'n-f5', 'n-redis3')}
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>`;
}

// ── 幾何檢查 ────────────────────────────────────────────

/** 把 XML 讀成 id → {parent, x, y, w, h, edge} 的表。 */
function readCells(xml) {
  const doc = parse(xml);
  const cells = new Map();
  for (const cell of doc.getElementsByTagName('mxCell')) {
    const holder = cell.parentNode;
    const id = cell.getAttribute('id') || (holder && holder.getAttribute('id'));
    if (!id) continue;
    const geo = cell.querySelector(':scope > mxGeometry');
    cells.set(id, {
      id,
      loomId: holder && holder.getAttribute ? holder.getAttribute('loomId') : null,
      label: holder && holder.getAttribute ? holder.getAttribute('label') : null,
      parent: cell.getAttribute('parent'),
      edge: cell.getAttribute('edge') === '1',
      vertex: cell.getAttribute('vertex') === '1',
      x: geo ? Number(geo.getAttribute('x') || 0) : 0,
      y: geo ? Number(geo.getAttribute('y') || 0) : 0,
      w: geo ? Number(geo.getAttribute('width') || 0) : 0,
      h: geo ? Number(geo.getAttribute('height') || 0) : 0,
      points: geo ? geo.getElementsByTagName('mxPoint').length : 0,
    });
  }
  return cells;
}

const overlaps = (a, b) =>
  a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

/**
 * 找出排版把圖弄壞的地方。回傳一串人看得懂的問題描述。
 *
 * 只檢查客觀的「壞掉」，不評美醜——美醜看 PNG。
 */
function findBreakage(cells, expectedParents) {
  const problems = [];

  for (const [id, want] of Object.entries(expectedParents)) {
    const got = cells.get(id);
    if (!got) {
      problems.push(`${id} 不見了`);
    } else if (got.parent !== want) {
      problems.push(`${id} 的父節點被改成 ${got.parent}（原本 ${want}）`);
    }
  }

  // 子節點必須留在父框內。座標是相對父節點的，所以直接比就好。
  for (const c of cells.values()) {
    if (!c.vertex) continue;
    const p = cells.get(c.parent);
    if (!p || !p.vertex) continue;
    if (c.x < 0 || c.y < 0 || c.x + c.w > p.w || c.y + c.h > p.h) {
      problems.push(
        `${c.label || c.id} 跑出 ${p.label || p.id} 的框外` +
        `（子 ${c.x},${c.y} ${c.w}×${c.h}｜父 ${p.w}×${p.h}）`
      );
    }
  }

  // 同一個父節點底下的兄弟不可以疊在一起。
  const byParent = new Map();
  for (const c of cells.values()) {
    if (!c.vertex) continue;
    if (!byParent.has(c.parent)) byParent.set(c.parent, []);
    byParent.get(c.parent).push(c);
  }
  for (const siblings of byParent.values()) {
    for (let i = 0; i < siblings.length; i++) {
      for (let j = i + 1; j < siblings.length; j++) {
        if (overlaps(siblings[i], siblings[j])) {
          problems.push(`${siblings[i].label || siblings[i].id} 與 ${siblings[j].label || siblings[j].id} 重疊`);
        }
      }
    }
  }

  return problems;
}

const EXPECTED_PARENTS = {
  'n-dc1': '1',
  'n-dc2': '1',
  'n-f5': '1',
  'n-vm1': 'n-dc1',
  'n-vm2': 'n-dc1',
  'n-vm3': 'n-dc1',
  'n-vm4': 'n-dc2',
  'n-api': 'n-vm1',
  'n-redis1': 'n-vm2',
  'n-redis2': 'n-vm3',
  'n-redis3': 'n-vm4',
};

// draw.io 給 ELK 排版的預設值（`ElkLayout.DEFAULTS`）。抄在這裡是因為
// 第一輪發現：`layered` 有帶 hierarchyHandling 跟 ORTHOGONAL，`mrtree` / `force`
// 兩個都沒有——這就是後兩者容器留白、邊走斜線的原因。
const 直角邊 = { edgeStyle: 'orthogonalEdgeStyle', corners: 'rounded' };
const 貫穿階層 = { 'elk.hierarchyHandling': 'INCLUDE_CHILDREN', 'elk.edgeRouting': 'ORTHOGONAL' };

const elk = (layout, config) => [{ layout, config: { ...config } }];

const PRESETS = [
  ['baseline', null],

  // 第一輪：畫面上那六個 preset 直接叫。
  ['elk-horizontalFlow', 'horizontalFlow'],
  ['elk-verticalFlow', 'verticalFlow'],
  ['elk-horizontalTree', 'horizontalTree'],
  ['elk-organic', 'organic'],
  ['drawio-hierarchical', [{ layout: 'mxHierarchicalLayout', config: { orientation: 'north' } }]],

  // 第二輪：手動補上 preset 沒帶的選項，看能不能救回來。
  ['tuned-mrtree', elk('elkTree', {
    'elk.direction': 'RIGHT', 'elk.spacing.nodeNode': '20', ...貫穿階層, ...直角邊,
  })],
  ['tuned-force', elk('elkOrganic', {
    'elk.force.iterations': '300', ...貫穿階層, ...直角邊,
  })],

  // 選單上沒有、但引擎支援的 stress。
  ['elk-stress', elk('elkStress', { ...貫穿階層, ...直角邊 })],

  // mrtree 的容器留白，看 resizeNodes / groupPadding 能不能收乾淨。
  ['tuned-mrtree-resize', elk('elkTree', {
    'elk.direction': 'RIGHT', 'elk.spacing.nodeNode': '20', resizeNodes: true, ...直角邊,
  })],
  ['tuned-mrtree-groupPadding', elk('elkTree', {
    'elk.direction': 'RIGHT', resizeNodes: true, groupPadding: 16, ...直角邊,
  })],

  // 容器內距：機房/ 機器的標題列需要上方留空間，否則標題會壓到子節點。
  ['tuned-layered-padding', elk('elkLayered', {
    'elk.algorithm': 'layered',
    'elk.direction': 'RIGHT',
    'elk.edgeRouting': 'ORTHOGONAL',
    'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
    'elk.padding': '[top=44,left=16,bottom=16,right=16]',
    'elk.spacing.nodeNode': '40',
    'elk.layered.spacing.nodeNodeBetweenLayers': '60',
    ...直角邊,
  })],
];

// ── 主流程 ──────────────────────────────────────────────

async function runNested(target) {
  const src = 'drawio://localhost/index.html'
    + '?embed=1&proto=json&configure=1&stealth=1&spin=1&noSaveBtn=1&noExitBtn=1&libraries=1';
  document.getElementById('target').textContent = src + '（巢狀）';

  const loaded = new Promise((resolve, reject) => {
    editor.addEventListener('load', () => resolve(), { once: true });
    setTimeout(() => reject(new Error('iframe 30 秒內沒有 load')), 30000);
  });
  editor.src = src;
  await loaded;

  await onEvent('configure', 30000);
  send({ action: 'configure', config: { compressXml: false } });
  await onEvent('init', 30000);
  record('編輯器就緒', true);

  const fixture = nestedFixture();
  invoke('dump', { name: 'nested-0-fixture.xml', contents: fixture });

  for (const [name, spec] of PRESETS) {
    // 每種排版都從乾淨的素材重來，免得互相污染。
    inbox.length = 0;
    send({ action: 'load', xml: fixture, autosave: 1 });
    await onEvent('load');

    if (spec !== null) {
      send({ action: 'layout', layouts: spec });
      try {
        await onEvent('autosave', 15000);
      } catch (e) {
        record(`${name}：排版沒有回應`, false, e.message);
        continue;
      }
    }

    send({ action: 'export', format: 'xml' });
    const out = await onEvent('export');
    const xml = out.xml || '';
    invoke('dump', { name: `nested-${name}.xml`, contents: xml });

    // 這是給人眼看的。程式判斷不了「好不好看」。
    try {
      send({ action: 'export', format: 'png', scale: 1.5 });
      const png = await onEvent('export', 20000);
      invoke('dump', { name: `nested-${name}.png.b64`, contents: String(png.data || '') });
    } catch (e) {
      log(`${name} 的 PNG 匯出失敗：${e.message}`);
    }

    const cells = readCells(xml);
    const missing = missingNestedLoomIds(xml);
    const problems = findBreakage(cells, EXPECTED_PARENTS);
    const bends = [...cells.values()].filter((c) => c.edge).reduce((n, c) => n + c.points, 0);
    const root = [...cells.values()].filter((c) => c.vertex && c.parent === '1');
    const extent = root.reduce(
      (b, c) => ({ w: Math.max(b.w, c.x + c.w), h: Math.max(b.h, c.y + c.h) }),
      { w: 0, h: 0 }
    );

    record(
      `${name}：結構沒被弄壞`,
      problems.length === 0 && missing.length === 0,
      [
        problems.length ? problems.slice(0, 3).join('；') : '巢狀關係、框內、不重疊都過',
        missing.length ? `loomId 掉了：${missing.join('、')}` : '',
        `外框 ${Math.round(extent.w)}×${Math.round(extent.h)}，轉彎點 ${bends} 個`,
      ].filter(Boolean).join(' ｜ ')
    );
  }

  await runWorkarounds();
}

/**
 * 兩個 workaround 的實測。
 *
 * 這兩件事都對應到已經寫進決策的需求：「套用排版不可以洗掉使用者手工排的版面」。
 * 如果 draw.io 本來就給得起，我們就不必自己做快照與還原。
 */
async function runWorkarounds() {
  const 排版 = elk('elkLayered', {
    'elk.direction': 'RIGHT', 'elk.edgeRouting': 'ORTHOGONAL',
    'elk.hierarchyHandling': 'INCLUDE_CHILDREN', ...直角邊,
  });

  // 排版有可能整個不回應（某些 ELK 選項組合會讓它默默死掉，不送任何錯誤事件），
  // 所以逾時要當成一種結果，不能讓整條流程斷掉。
  const 跑一輪 = async (fixture, layouts) => {
    inbox.length = 0;
    send({ action: 'load', xml: fixture, autosave: 1 });
    await onEvent('load');
    send({ action: 'layout', layouts });
    try {
      await onEvent('autosave', 15000);
    } catch {
      return null;
    }
    send({ action: 'export', format: 'xml' });
    return readCells((await onEvent('export')).xml || '');
  };

  // ① style 帶 movable=0 的節點，排版時應該原地不動。
  //    applier 會給它 elk.position，但 ELK 只有在 elk.interactive 為真時才理會。
  const pinned = nestedFixture({ pinF5: true });
  const before = readCells(pinned).get('n-f5');
  const after = (await 跑一輪(pinned, 排版)).get('n-f5');
  record(
    'movable=0 單獨用可以釘住節點',
    Boolean(after) && after.x === before.x && after.y === before.y,
    `F5 ${before.x},${before.y} → ${after ? `${after.x},${after.y}` : '不見了'}`
  );

  // ELK 的 elk.position 只有在各階段策略都切成 INTERACTIVE 時才會被理會，
  // 光開 elk.interactive 不夠。
  const after2 = (await 跑一輪(pinned, elk('elkLayered', {
    'elk.direction': 'RIGHT', 'elk.edgeRouting': 'ORTHOGONAL',
    'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
    'elk.interactive': 'true',
    'elk.layered.cycleBreaking.strategy': 'INTERACTIVE',
    'elk.layered.layering.strategy': 'INTERACTIVE',
    'elk.layered.crossingMinimization.strategy': 'INTERACTIVE',
    'elk.layered.nodePlacement.strategy': 'INTERACTIVE',
    ...直角邊,
  })));
  record(
    'movable=0 加上整套 INTERACTIVE 策略可以釘住節點',
    Boolean(after2) && after2.get('n-f5')
      && after2.get('n-f5').x === before.x && after2.get('n-f5').y === before.y,
    after2 === null
      ? '排版整個沒回應——ELK 文件警告過 INTERACTIVE 與 INCLUDE_CHILDREN 不相容，實測是默默死掉、不送任何錯誤事件'
      : `F5 ${before.x},${before.y} → ${after2.get('n-f5').x},${after2.get('n-f5').y}`
  );

  // ② rootCellIds 應該只重排指定的那一塊，其他地方一動也不動。
  //    沒有 preserveOrigin 的話，排完的那一塊會被搬到原點，等於整張圖都位移。
  const plain = nestedFixture();
  const base = readCells(plain);
  const scoped = await 跑一輪(plain, elk('elkLayered', {
    'elk.direction': 'RIGHT', rootCellIds: ['n-dc1'], preserveOrigin: true, ...直角邊,
  }));
  const 沒動 = ['n-dc2', 'n-f5'].filter((id) => {
    const a = base.get(id);
    const b = scoped.get(id);
    return b && a.x === b.x && a.y === b.y;
  });
  const 有動 = ['n-vm1', 'n-vm2', 'n-vm3'].filter((id) => {
    const a = base.get(id);
    const b = scoped.get(id);
    return b && (a.x !== b.x || a.y !== b.y);
  });
  // 再跟「完全不給 rootCellIds」比一次：如果結果一模一樣，代表這個鍵被整個忽略。
  const 全圖 = await 跑一輪(plain, elk('elkLayered', { 'elk.direction': 'RIGHT', ...直角邊 }));
  const 位置相同 = ['n-dc1', 'n-dc2', 'n-f5', 'n-vm1'].every((id) => {
    const a = scoped.get(id);
    const b = 全圖.get(id);
    return a && b && a.x === b.x && a.y === b.y;
  });
  record(
    'rootCellIds 能把排版限定在單一容器內',
    沒動.length === 2 && 有動.length > 0,
    `框外沒動的：${沒動.join('、') || '無'}｜框內有動的：${有動.join('、') || '無'}`
      + `｜與不給 rootCellIds 的結果${位置相同 ? '完全相同（等於被忽略）' : '不同'}`
  );
}

function missingNestedLoomIds(xml) {
  const found = new Set();
  for (const m of xml.matchAll(/loomId="([^"]+)"/g)) found.add(m[1]);
  return Object.entries(NESTED).filter(([, id]) => !found.has(id)).map(([k]) => k);
}
