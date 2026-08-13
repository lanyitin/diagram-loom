// 階段 0 的探針。
//
// 這支腳本會自己跑完一輪 draw.io 嵌入協定，把每一項檢查的結果回報給 Rust，
// 然後結束程式。目的是讓「能不能走」這個問題有一個不需要人盯著看的答案。
//
// 檢查的東西照風險排序，最前面的失敗會讓後面的失去意義：
//   1. iframe 在 Tauri 的 CSP 底下到底載不載得起來
//   2. postMessage 的握手（configure / init）通不通
//   3. 載入、匯出、存檔的往返
//   4. loomId 自訂屬性能不能活過一次真正的模型異動（套排版）

const STEP_TIMEOUT = 15000;
const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);

// ── 測試素材 ────────────────────────────────────────────
// 兩個服務落地 + 一條連線，全部掛 loomId，長得像我們真的會產出的東西。

const LOOM_IDS = {
  api: '11111111-1111-1111-1111-111111111111',
  redis: '22222222-2222-2222-2222-222222222222',
  conn: '33333333-3333-3333-3333-333333333333',
};

const FIXTURE = `<mxfile host="loom">
  <diagram id="prod-main" name="prod">
    <mxGraphModel dx="800" dy="600" grid="1" gridSize="10" page="1" pageWidth="1169" pageHeight="826">
      <root>
        <mxCell id="0"/>
        <mxCell id="1" parent="0"/>
        <object label="order-api" loomId="${LOOM_IDS.api}" loomKind="containerInstance" id="n-api">
          <mxCell style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;" vertex="1" parent="1">
            <mxGeometry x="40" y="40" width="160" height="60" as="geometry"/>
          </mxCell>
        </object>
        <object label="redis-01" loomId="${LOOM_IDS.redis}" loomKind="containerInstance" id="n-redis">
          <mxCell style="rounded=1;whiteSpace=wrap;html=1;fillColor=#d5e8d4;" vertex="1" parent="1">
            <mxGeometry x="40" y="40" width="160" height="60" as="geometry"/>
          </mxCell>
        </object>
        <object label="快取讀寫" loomId="${LOOM_IDS.conn}" loomKind="connection" id="e-cache">
          <mxCell style="edgeStyle=orthogonalEdgeStyle;html=1;" edge="1" parent="1" source="n-api" target="n-redis">
            <mxGeometry relative="1" as="geometry"/>
          </mxCell>
        </object>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>`;

// ── 畫面 ────────────────────────────────────────────────

const checks = [];
const $checks = document.getElementById('checks');
const $log = document.getElementById('log');
const editor = document.getElementById('editor');

function log(line) {
  $log.textContent += line + '\n';
  $log.scrollTop = $log.scrollHeight;
  console.log('[probe]', line);
}

function record(name, ok, detail = '') {
  checks.push({ name, ok, detail: String(detail).slice(0, 400) });
  const li = document.createElement('li');
  li.className = ok ? 'pass' : 'fail';

  const mark = document.createElement('span');
  mark.className = 'mark';
  mark.textContent = ok ? 'OK' : '××';

  const body = document.createElement('span');
  body.textContent = name;

  const note = document.createElement('div');
  note.className = 'detail';
  note.textContent = detail;

  body.appendChild(note);
  li.append(mark, body);
  $checks.appendChild(li);
}

// ── 訊息等待 ────────────────────────────────────────────
// draw.io 的事件不一定照我們期待的順序來，所以收到的都先進 buffer，
// 等待時先掃 buffer 再等新的。

const inbox = [];
const pending = [];

window.addEventListener('message', (e) => {
  if (e.source !== editor.contentWindow) return;
  let msg;
  try {
    msg = typeof e.data === 'string' ? JSON.parse(e.data) : e.data;
  } catch {
    log('收到非 JSON 訊息：' + String(e.data).slice(0, 80));
    return;
  }
  log('◀ ' + (msg.event || msg.action || '?'));
  inbox.push(msg);
  drain();
});

function drain() {
  for (let i = pending.length - 1; i >= 0; i--) {
    const w = pending[i];
    const hit = inbox.findIndex((m) => w.match(m));
    if (hit >= 0) {
      const [msg] = inbox.splice(hit, 1);
      pending.splice(i, 1);
      clearTimeout(w.timer);
      w.resolve(msg);
    }
  }
}

function waitFor(label, match, timeout = STEP_TIMEOUT) {
  return new Promise((resolve, reject) => {
    const w = { match, resolve };
    w.timer = setTimeout(() => {
      pending.splice(pending.indexOf(w), 1);
      reject(new Error(`等 ${label} 超過 ${timeout}ms`));
    }, timeout);
    pending.push(w);
    drain();
  });
}

const onEvent = (name, timeout) => waitFor(name, (m) => m.event === name, timeout);

function send(msg) {
  log('▶ ' + (msg.action || '?'));
  editor.contentWindow.postMessage(JSON.stringify(msg), '*');
}

// ── XML 觀察 ────────────────────────────────────────────

function parse(xml) {
  const doc = new DOMParser().parseFromString(xml, 'text/xml');
  if (doc.querySelector('parsererror')) throw new Error('XML 解析失敗');
  return doc;
}

/** 從（可能被壓縮過的）存檔內容取出所有 loomId。 */
function loomIdsIn(xml) {
  const found = new Set();
  for (const m of xml.matchAll(/loomId="([^"]+)"/g)) found.add(m[1]);
  return found;
}

function missingLoomIds(xml) {
  const found = loomIdsIn(xml);
  return Object.entries(LOOM_IDS).filter(([, id]) => !found.has(id)).map(([k]) => k);
}

/** 取出某個 loomId 對應形狀的座標，用來判斷排版有沒有真的動到東西。 */
function geometryOf(xml, loomId) {
  const doc = parse(xml);
  const obj = [...doc.getElementsByTagName('object')].find((o) => o.getAttribute('loomId') === loomId);
  if (!obj) return null;
  const geo = obj.querySelector('mxGeometry');
  if (!geo) return null;
  return { x: Number(geo.getAttribute('x')), y: Number(geo.getAttribute('y')) };
}

// ── 主流程 ──────────────────────────────────────────────

async function run(target) {
  // 兩個目標走同一個自訂協定，只有 Rust 那端的根目錄不同——
  // 這樣對照組跟正式組的差異只會來自內容，不會來自管路。
  const query = target === 'stub'
    ? '?embed=1&proto=json&configure=1'
    : '?embed=1&proto=json&configure=1&stealth=1&spin=1&noSaveBtn=1&noExitBtn=1&libraries=1';
  const src = 'drawio://localhost/index.html' + query;

  document.getElementById('target').textContent = src;
  log('目標：' + src);

  // 1 ── iframe 在 CSP 底下載不載得起來
  const loaded = new Promise((resolve, reject) => {
    editor.addEventListener('load', () => resolve(), { once: true });
    editor.addEventListener('error', () => reject(new Error('iframe error 事件')), { once: true });
    setTimeout(() => reject(new Error('iframe 30 秒內沒有 load')), 30000);
  });
  editor.src = src;
  await loaded;
  record('iframe 在 Tauri CSP 下載入', true, src);

  // 2 ── 握手。收得到 configure 就代表 iframe 內的 JS 真的執行了。
  await onEvent('configure', 30000);
  record('iframe 內的 JS 執行（收到 configure）', true, 'CSP 沒有擋掉 draw.io 的腳本');
  send({
    action: 'configure',
    config: { defaultFonts: ['PingFang TC'], compressXml: false },
  });

  await onEvent('init', 30000);
  record('嵌入協定握手完成（收到 init）', true);

  // 3 ── 載入我們產的 XML
  send({ action: 'load', xml: FIXTURE, autosave: 1 });
  const loadEvt = await onEvent('load');
  record('載入我們產生的 XML', true, `回傳 ${String(loadEvt.xml || '').length} 字元`);

  // 4a ── 匯出圖片。這是「放進文件與簡報」那條路。
  send({ action: 'export', format: 'xmlsvg' });
  const asSvg = await onEvent('export');
  const isSvg = String(asSvg.data || '').startsWith('data:image/svg+xml');
  record('匯出 SVG 圖片', isSvg, isSvg ? `${asSvg.data.length} 字元的 data URI` : String(asSvg.data).slice(0, 80));
  invoke('dump', { name: `${target}-1-xmlsvg.xml`, contents: asSvg.xml || '' });

  // xmlsvg 附帶的 xml 欄位是壓縮過的——不是設定問題，是 draw.io 在這條路上寫死的。
  // 記成一項檢查，免得日後有人拿它當模型來源又踩一次。
  const svgXmlCompressed = !String(asSvg.xml || '').includes('<mxGraphModel');
  record('xmlsvg 附帶的 xml 是壓縮的（已知行為）', svgXmlCompressed,
    'draw.io 在這條路寫死 uncompressed=false，取模型不能用它');

  // 4b ── 取模型。跟取圖片是兩條不同的路。
  send({ action: 'export', format: 'xml' });
  const asXml = await onEvent('export');
  const exportedXml = asXml.xml || '';
  invoke('dump', { name: `${target}-2-model.xml`, contents: exportedXml });
  record('format:xml 取回未壓縮的模型 XML', exportedXml.includes('<mxGraphModel'),
    `${exportedXml.length} 字元`);

  const missingAfterExport = missingLoomIds(exportedXml);
  record('匯出的 XML 保留 loomId', missingAfterExport.length === 0,
    missingAfterExport.length ? '掉了：' + missingAfterExport.join('、') : '三個都在');
  record('匯出的 XML 保留 loomKind', exportedXml.includes('loomKind="containerInstance"'),
    '自訂屬性不只一個欄位能活');

  const before = geometryOf(exportedXml, LOOM_IDS.api);
  log('排版前 order-api 座標：' + JSON.stringify(before));

  // 5 ── 套用排版。這是真正會改動模型的操作，
  //      也是「排版只改座標、不動 loomId」這個假設的檢驗點。
  //
  //      兩種都試：draw.io 原生的階層排版，以及它自己包進去的 ELK。
  //      如果 ELK 這條通，我們就不必自己抱一份 elkjs 進來。
  const layoutXml = {};

  for (const [label, spec] of [
    ['draw.io 原生階層排版', [{ layout: 'mxHierarchicalLayout', config: { orientation: 'west', intraCellSpacing: 80 } }]],
    ['draw.io 內建的 ELK（horizontalFlow）', 'horizontalFlow'],
  ]) {
    inbox.length = 0;
    send({ action: 'layout', layouts: spec });
    try {
      const auto = await onEvent('autosave', 12000);
      layoutXml[label] = auto.xml;
      record(`${label} 可由 postMessage 觸發`, true, '排版後自動送出 autosave');
    } catch (e) {
      record(`${label} 可由 postMessage 觸發`, false, e.message);
    }
  }

  // 6 ── 隨時取回目前的 XML。
  //      注意：draw.io v31 已經沒有 save 這個 action / event 了，
  //      存檔時要對帳就得靠 export（或改動時自動來的 autosave）。
  send({ action: 'export', format: 'xml' });
  const finalExport = await onEvent('export');
  const finalXml = finalExport.xml || layoutXml['draw.io 內建的 ELK（horizontalFlow）'] || '';
  invoke('dump', { name: `${target}-3-final.xml`, contents: finalXml });
  invoke('dump', { name: `${target}-4-load.xml`, contents: loadEvt.xml || '' });
  record('編輯後可隨時用 export 取回 XML', finalXml.length > 0, `${finalXml.length} 字元`);

  const missingAtEnd = missingLoomIds(finalXml);
  record('編輯往返後 loomId 全數存活', missingAtEnd.length === 0,
    missingAtEnd.length ? '掉了：' + missingAtEnd.join('、') : '三個都在——對帳的前提成立');

  const after = geometryOf(finalXml, LOOM_IDS.api);
  const moved = before && after && (before.x !== after.x || before.y !== after.y);
  record('排版確實改動了座標', Boolean(moved),
    `${JSON.stringify(before)} → ${JSON.stringify(after)}`);

  const compressed = finalXml.includes('<diagram') && !finalXml.includes('<mxGraphModel');
  record('存出的是未壓縮 XML（可讀 diff）', !compressed,
    compressed ? 'compressXml 設定沒吃到，存成 base64 了' : 'configure 的 compressXml:false 有生效');
}

// ── 進入點 ──────────────────────────────────────────────

(async () => {
  let target = 'drawio';
  try {
    target = await invoke('target_command');
  } catch (e) {
    log('取不到 target，預設走 drawio：' + e);
  }

  let scenario = 'flat';
  try {
    scenario = await invoke('scenario');
  } catch (e) {
    log('取不到 scenario，預設走 flat：' + e);
  }

  try {
    const 情境 = { nested: runNested, scale: runScale };
    await (情境[scenario] ? 情境[scenario](target) : run(target));
  } catch (e) {
    record('流程中斷', false, e.message || String(e));
    log('中斷：' + (e.stack || e));
  }

  try {
    await invoke('report', { target, checks });
  } catch (e) {
    log('回報失敗：' + e);
  }
})();
