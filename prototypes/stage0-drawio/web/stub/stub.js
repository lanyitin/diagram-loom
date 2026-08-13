// 假 draw.io：只實作嵌入協定中我們會用到的部分，用來當對照組。
//
// 它刻意讓 XML 走一遍 DOMParser → XMLSerializer，因為「自訂屬性能不能活過
// 一次序列化」正是我們要驗的事；如果連這支都掉 loomId，那是我們的 XML 寫壞了。

const trace = document.getElementById('trace');
const say = (t) => { trace.textContent += t + '\n'; };

let xml = '';

function reply(msg) {
  say('▶ ' + (msg.event || '?'));
  parent.postMessage(JSON.stringify(msg), '*');
}

function reserialize(input) {
  const doc = new DOMParser().parseFromString(input, 'text/xml');
  return new XMLSerializer().serializeToString(doc);
}

/** 模仿 draw.io 壓縮過的 mxfile：內容被塞進 base64，看不到 loomId。 */
function fakeCompressed(input) {
  const body = btoa(unescape(encodeURIComponent(input)));
  return `<mxfile host="stub"><diagram id="d" name="p">${body}</diagram></mxfile>`;
}

let layoutRuns = 0;

/** 把每個形狀往右挪，模擬一次真正會改動模型的排版。 */
function fakeLayout() {
  layoutRuns++;
  const doc = new DOMParser().parseFromString(xml, 'text/xml');
  let step = 0;
  for (const geo of doc.getElementsByTagName('mxGeometry')) {
    if (geo.getAttribute('x') === null) continue;
    geo.setAttribute('x', String(200 + step * 240 + layoutRuns * 17));
    geo.setAttribute('y', String(120 * layoutRuns));
    step++;
  }
  xml = new XMLSerializer().serializeToString(doc);
}

window.addEventListener('message', (e) => {
  let msg;
  try {
    msg = typeof e.data === 'string' ? JSON.parse(e.data) : e.data;
  } catch {
    return;
  }
  say('◀ ' + (msg.action || '?'));

  switch (msg.action) {
    case 'configure':
      reply({ event: 'init' });
      break;
    case 'load':
      xml = reserialize(msg.xml || '');
      reply({ event: 'load', xml });
      break;
    case 'export':
      // 照抄真的 draw.io 的行為：xmlsvg 附帶的 xml 一定是壓縮的，
      // 只有 format:'xml' 才給得到看得懂的模型。
      reply({
        event: 'export',
        format: msg.format,
        data: 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg"/>'),
        xml: msg.format === 'xml' ? xml : fakeCompressed(xml),
      });
      break;
    case 'layout':
      fakeLayout();
      reply({ event: 'autosave', xml });
      break;
    // 真的 draw.io v31 沒有 save 這個 action，這裡也刻意不提供。
    default:
      say('（不認識的 action）');
  }
});

// 真的 draw.io 在 configure=1 時會先問設定，再送 init。照抄這個順序。
reply({ event: 'configure' });
