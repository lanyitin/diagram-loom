// 規模測試。
//
// 真實情境是「數百條連線」，而巢狀那一輪只有 11 個節點——等於沒測到規模。
// 這支從小到大跑一梯，量到它撐不住為止，找出「幾個節點開始不能用」。
//
// 產生的圖跟真實的 Deployment 圖同形狀：
//
//   機房 ─┬─ 機器 ─┬─ 服務（client）
//         │        └─ 服務（server）
//         └─ ...
//   每個機房一台 F5，client → F5 → 該機房的 server，另外拉一部分跨機房的線。
//
// 量四件事，因為使用者感覺得到的卡頓不一定出在排版：
//   載入 XML／套排版／取回 XML／XML 有多大

const SCALE_LADDER = [
  { dcs: 3, machines: 3, services: 2 },
  { dcs: 6, machines: 4, services: 2 },
  { dcs: 10, machines: 6, services: 2 },
  { dcs: 15, machines: 8, services: 2 },
  { dcs: 20, machines: 10, services: 3 },
  { dcs: 30, machines: 12, services: 3 },
  { dcs: 40, machines: 15, services: 3 },
];

/** 單一步驟超過這個秒數就不用再往上爬了——使用者早就關掉了。 */
const 放棄門檻 = 90000;

const SCALE_LAYOUT = [{
  layout: 'elkLayered',
  config: {
    'elk.algorithm': 'layered',
    'elk.direction': 'RIGHT',
    'elk.edgeRouting': 'ORTHOGONAL',
    'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
    'elk.padding': '[top=44,left=16,bottom=16,right=16]',
    'elk.spacing.nodeNode': '40',
    'elk.layered.spacing.nodeNodeBetweenLayers': '60',
    edgeStyle: 'orthogonalEdgeStyle',
    corners: 'rounded',
  },
}];

/**
 * 產生一張指定規模的 Deployment 圖。
 *
 * 每個元素都有 loomId，因為我們想順便知道「幾百個自訂屬性」會不會拖慢序列化。
 */
function scaleFixture({ dcs, machines, services }) {
  const parts = [];
  const clients = [];
  const servers = [];
  let n = 0;
  const uid = (p) => `${p}-${String(++n).padStart(6, '0')}`;

  const box = (id, label, kind, parent, x, y, w, h, style) => {
    parts.push(
      `<object label="${label}" loomId="${uid('id')}" loomKind="${kind}" id="${id}">` +
      `<mxCell style="${style}" vertex="1" parent="${parent}">` +
      `<mxGeometry x="${x}" y="${y}" width="${w}" height="${h}" as="geometry"/>` +
      `</mxCell></object>`
    );
  };

  for (let d = 0; d < dcs; d++) {
    const dcId = `dc${d}`;
    box(dcId, `機房-${d + 1}`, 'deploymentNode', '1', 40 + d * 900, 40,
      machines * 240 + 40, services * 100 + 120,
      'rounded=0;whiteSpace=wrap;html=1;container=1;collapsible=0;verticalAlign=top;fillColor=#f5f5f5;');

    box(`f5-${d}`, `F5-${d + 1}`, 'infrastructureNode', '1', 40 + d * 900, 400, 140, 80,
      'rhombus;whiteSpace=wrap;html=1;fillColor=#ffe6cc;');

    for (let m = 0; m < machines; m++) {
      const vmId = `vm${d}-${m}`;
      box(vmId, `vm-${d + 1}-${m + 1}`, 'deploymentNode', dcId, 20 + m * 240, 50,
        200, services * 90 + 20,
        'rounded=0;whiteSpace=wrap;html=1;container=1;collapsible=0;verticalAlign=top;fillColor=#e8e8e8;');

      for (let s = 0; s < services; s++) {
        const svcId = `svc${d}-${m}-${s}`;
        // 每台機器的第一個服務當 client，其餘當 server。
        const isClient = s === 0;
        box(svcId, `${isClient ? 'app' : 'redis'}-${d + 1}${m + 1}${s + 1}`,
          'containerInstance', vmId, 20, 20 + s * 90, 160, 60,
          `rounded=1;whiteSpace=wrap;html=1;fillColor=${isClient ? '#dae8fc' : '#d5e8d4'};`);
        (isClient ? clients : servers).push({ id: svcId, dc: d });
      }
    }
  }

  const edge = (from, to, label) => {
    parts.push(
      `<object label="${label}" loomId="${uid('id')}" loomKind="connection" id="e${++n}">` +
      `<mxCell style="edgeStyle=orthogonalEdgeStyle;html=1;" edge="1" parent="1" ` +
      `source="${from}" target="${to}"><mxGeometry relative="1" as="geometry"/>` +
      `</mxCell></object>`
    );
  };

  for (const c of clients) {
    edge(c.id, `f5-${c.dc}`, '送進 VIP');
  }
  // F5 分流到同機房的 server；另外每個機房拉兩條跨機房的線，製造真實的複雜度。
  for (let d = 0; d < dcs; d++) {
    for (const s of servers.filter((x) => x.dc === d)) {
      edge(`f5-${d}`, s.id, '分流');
    }
    const 對面 = servers.filter((x) => x.dc === (d + 1) % dcs).slice(0, 2);
    for (const s of 對面) edge(`f5-${d}`, s.id, '跨機房備援');
  }

  const xml = `<mxfile host="loom"><diagram id="scale" name="prod">` +
    `<mxGraphModel dx="2000" dy="1400" grid="0" page="1" pageWidth="1169" pageHeight="826">` +
    `<root><mxCell id="0"/><mxCell id="1" parent="0"/>${parts.join('')}</root>` +
    `</mxGraphModel></diagram></mxfile>`;

  const vertices = dcs * (1 + 1 + machines * (1 + services));
  const edges = clients.length + servers.length + dcs * 2;
  return { xml, vertices, edges };
}

/** 排完之後整張圖佔多大。只看頂層節點就夠——子節點都在容器裡面。 */
function 畫布外框(xml) {
  const cells = readCells(xml);
  let w = 0;
  let h = 0;
  for (const c of cells.values()) {
    if (!c.vertex || c.parent !== '1') continue;
    w = Math.max(w, c.x + c.w);
    h = Math.max(h, c.y + c.h);
  }
  return { w: Math.round(w), h: Math.round(h) };
}

async function runScale() {
  const src = 'drawio://localhost/index.html'
    + '?embed=1&proto=json&configure=1&stealth=1&spin=1&noSaveBtn=1&noExitBtn=1&libraries=1';
  document.getElementById('target').textContent = src + '（規模）';

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

  const rows = [];

  for (const size of SCALE_LADDER) {
    const { xml, vertices, edges } = scaleFixture(size);
    const 規模 = `${vertices} 節點 / ${edges} 連線`;
    inbox.length = 0;

    const t0 = performance.now();
    send({ action: 'load', xml, autosave: 1 });
    try {
      await onEvent('load', 放棄門檻);
    } catch (e) {
      record(`${規模}：載入`, false, `超過 ${放棄門檻 / 1000}s 沒回應`);
      break;
    }
    const 載入 = performance.now() - t0;

    const t1 = performance.now();
    send({ action: 'layout', layouts: SCALE_LAYOUT });
    let 排版;
    try {
      await onEvent('autosave', 放棄門檻);
      排版 = performance.now() - t1;
    } catch (e) {
      record(`${規模}：排版`, false, `超過 ${放棄門檻 / 1000}s 沒回應——這個規模不能用`);
      break;
    }

    const t2 = performance.now();
    send({ action: 'export', format: 'xml' });
    const out = await onEvent('export', 放棄門檻);
    const 取回 = performance.now() - t2;
    const 大小 = (out.xml || '').length;

    // 只在最大的那一梯存檔，免得倒出一堆幾 MB 的檔。
    if (size === SCALE_LADDER[SCALE_LADDER.length - 1]) {
      invoke('dump', { name: 'scale-largest.xml', contents: out.xml || '' });
    }

    // 排完之後的畫布有多大。這比排版時間更可能是真正的牆——
    // 「產出可以直接放進文件與簡報的圖」是這個工具的目標之一。
    const 外框 = 畫布外框(out.xml || '');

    const ms = (v) => `${Math.round(v)}ms`;
    rows.push({ 規模, 載入, 排版, 取回, 大小, 外框 });
    record(
      `${規模}`,
      排版 < 10000,
      `載入 ${ms(載入)}｜排版 ${ms(排版)}｜取回 ${ms(取回)}｜XML ${Math.round(大小 / 1024)}KB`
        + `｜畫布 ${外框.w}×${外框.h}px`
        + (排版 >= 10000 ? '　← 排版超過 10 秒，使用者會以為當掉' : '')
    );

    if (排版 > 放棄門檻 * 0.6) {
      log('排版時間逼近門檻，不再往上爬');
      break;
    }
  }

  // 這個工具的目標之一是「產出可以直接放進文件與簡報的圖」。
  // 排版撐得住，不代表匯出撐得住——分開量。
  if (rows.length) {
    const 最大 = rows[rows.length - 1];
    for (const [格式, 上限] of [['xmlsvg', 120000], ['png', 120000]]) {
      const t = performance.now();
      try {
        send({ action: 'export', format: 格式 });
        const r = await onEvent('export', 上限);
        const 位元組 = String(r.data || '').length;
        invoke('dump', { name: `scale-largest-${格式}.b64`, contents: String(r.data || '') });
        record(
          `${最大.規模} 匯出 ${格式}`,
          位元組 > 0,
          `${Math.round(performance.now() - t)}ms，${Math.round(位元組 / 1024)}KB`
            + `（畫布 ${最大.外框.w}×${最大.外框.h}px）`
        );
      } catch (e) {
        record(`${最大.規模} 匯出 ${格式}`, false,
          `${Math.round(performance.now() - t)}ms 之後仍無回應——這個規模匯不出圖`);
      }
    }
  }

  // 把成長趨勢也講清楚：是線性還是爆炸？
  if (rows.length >= 2) {
    const a = rows[0];
    const b = rows[rows.length - 1];
    const 節點倍數 = parseInt(b.規模) / parseInt(a.規模);
    const 時間倍數 = b.排版 / a.排版;
    record(
      '排版時間的成長速度',
      時間倍數 < 節點倍數 * 3,
      `節點 ×${節點倍數.toFixed(1)} 時，排版時間 ×${時間倍數.toFixed(1)}`
        + `（${Math.round(a.排版)}ms → ${Math.round(b.排版)}ms）`
    );
  }
}
