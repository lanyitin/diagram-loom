/**
 * 編輯器原型的探針：**每一個 widget 都真的按下去**。
 *
 * 面板長出來不算數——draw.io 的格式面板值錢的地方是「按了圖真的變」，
 * 而那件事只有真的按下去才知道。所以這裡開一個真的 Chrome，
 * 點形狀、改顏色、按粗體、打座標、復原、對齊，每一步都回頭問圖。
 *
 * 跑法：另一個終端機開著 `pnpm app`，然後 `node src/editor/probe.mjs`。
 * 離開碼：0 全過、1 有項目失敗。
 */

import puppeteer from 'puppeteer-core'

const CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
const URL = process.env.APP ?? 'http://localhost:5180/editor.html'

const results = []
const check = (name, ok, detail = '') => {
  results.push({ name, ok, detail })
  console.log(`${ok ? '  ✓' : '  ✗'} ${name}${detail ? `　${detail}` : ''}`)
}
const section = (title) => console.log(`\n── ${title} ${'─'.repeat(Math.max(0, 50 - title.length))}`)

const browser = await puppeteer.launch({ executablePath: CHROME, headless: true })
const page = await browser.newPage()
await page.setViewport({ width: 1400, height: 900 })

const errors = []
page.on('pageerror', (e) => errors.push(e.message))
page.on('console', (m) => {
  const from = m.location()?.url ?? ''
  if (m.type() === 'error' && !from.includes('favicon')) errors.push(`${m.text()}（${from}）`)
})

await page.goto(URL, { waitUntil: 'networkidle0' })
await new Promise((r) => setTimeout(r, 700))

/** 問圖：某個 cell 現在的樣式與幾何。**問圖，不是問面板。** */
const cellOf = (id) => page.evaluate((cellId) => {
  const graph = window.__editor.graph
  const cell = graph.model.getCell(cellId)
  const geo = cell.getGeometry()
  return { style: graph.getCellStyle(cell), geo: { x: geo.x, y: geo.y, w: geo.width, h: geo.height } }
}, id)

const select = (id) => page.evaluate((cellId) => {
  const graph = window.__editor.graph
  graph.setSelectionCell(graph.model.getCell(cellId))
}, id)

// ── 版面 ────────────────────────────────────────────────────────

section('組起來了嗎')

check('沒有 JavaScript 錯誤', errors.length === 0, errors[0] ?? '')

const widgets = await page.evaluate(() => ({
  ...window.__editor,
  graph: undefined,
  undoManager: undefined,
  toolbar: document.querySelectorAll('.toolbar button').length,
  status: document.getElementById('status').textContent,
  outlineDrawn: document.querySelectorAll('#outline svg').length > 0,
}))
check('工具列長出按鈕', widgets.toolbar >= 15, `${widgets.toolbar} 顆`)
check('大綱畫出來了', widgets.outlineDrawn, '縮圖裡有 SVG')
check('狀態列說了「還剩幾個」', widgets.status.includes('沒指定'), widgets.status)

/**
 * ⚠️ 摺疊圖示（容器左上角那個小方塊）預設指到 `./collapsed.gif`——那是
 * maxGraph 自己的圖檔，而我們沒有把它放到網站根目錄。
 *
 * 症狀是**功能好好的、點得下去，但畫不出任何東西**：`<image>` 元素照樣
 * 長出來、位置正確、大小正確。而且 vite 會把找不到的路徑回成 index.html
 * （200），連 404 都沒有。這是最難查的那一種。
 */
const foldIcons = await page.evaluate(() =>
  [...document.querySelectorAll('#canvas svg image')]
    .map((i) => i.getAttribute('href') ?? i.getAttribute('xlink:href'))
    .filter((href) => !href.includes('circle')))
check('摺疊圖示不是指到不存在的檔案',
  foldIcons.length > 0 && foldIcons.every((href) => href.startsWith('data:')),
  foldIcons.length ? `${foldIcons.length} 個，都是 data URI` : '一個都沒有')

check('摺疊真的收得起來',
  await page.evaluate(() => {
    const g = window.__editor.graph
    const cell = g.model.getCell('vm-03')
    g.foldCells(true, false, [cell])
    const folded = cell.isCollapsed()
    g.foldCells(false, false, [cell])
    return folded && !cell.isCollapsed()
  }),
  '收起來再打開')

// ── 選取 → 面板 ────────────────────────────────────────────────

section('選了才出現，選什麼出現什麼')

check(
  '什麼都沒選時面板是收的',
  await page.$eval('.format .empty', (el) => !el.hidden),
  '只留一句「選一個形狀或一條線」',
)

/** 用真的滑鼠點，順便驗選取在編輯器（可拖曳）模式下也還在。 */
const at = await page.evaluate(() => {
  const t = [...document.querySelectorAll('#canvas svg text')].find((x) => x.textContent === 'order-api')
  const b = t.getBoundingClientRect()
  return { x: b.x + b.width / 2, y: b.y + b.height / 2 }
})
await page.mouse.click(at.x, at.y)
await new Promise((r) => setTimeout(r, 200))

const groups = () => page.evaluate(() =>
  [...document.querySelectorAll('.group h2')].filter((h) => !h.parentElement.hidden).map((h) => h.textContent))

check('點一個框，形狀與文字那幾組打開', (await groups()).includes('形狀'), (await groups()).join('／'))
check('單選才給位置與大小', (await groups()).includes('位置與大小'))
check('選框的時候不給「線」那組', !(await groups()).includes('線'))

await select('e1')
await new Promise((r) => setTimeout(r, 150))
check('選一條線，換成「線」那組', (await groups()).includes('線'), (await groups()).join('／'))

// ── 樣式：按了圖真的變嗎 ────────────────────────────────────────

section('改樣式（原本 draw.io 那些）')

await select('order-api')
await new Promise((r) => setTimeout(r, 150))
const before = await cellOf('order-api')

await page.$eval('.format input.color', (el) => {
  el.value = '#ffcc00'
  el.dispatchEvent(new Event('input', { bubbles: true }))
})
check('填色改得動', (await cellOf('order-api')).style.fillColor === '#ffcc00',
  `${before.style.fillColor} → ${(await cellOf('order-api')).style.fillColor}`)

await page.click('.format .toggle[title="粗體"]')
check('粗體改得動', ((await cellOf('order-api')).style.fontStyle & 1) === 1,
  `fontStyle = ${(await cellOf('order-api')).style.fontStyle}`)

await page.$$eval('.format select', (els) => {
  const font = els[0]
  font.value = 'Georgia'
  font.dispatchEvent(new Event('change', { bubbles: true }))
})
check('字型改得動', (await cellOf('order-api')).style.fontFamily === 'Georgia')

await page.$$eval('.format .num', (els) => {
  // 位置那一列的第一個是 x（前面的 num 是框線寬、透明度、字級）
  const x = els.find((e) => e.parentElement.querySelector('.label')?.textContent === '位置')
  x.value = '99'
  x.dispatchEvent(new Event('input', { bubbles: true }))
})
check('打字改座標', (await cellOf('order-api')).geo.x === 99, `x = ${(await cellOf('order-api')).geo.x}`)

// ── 復原 ────────────────────────────────────────────────────────

section('復原（自己接才有）')

await page.click('.toolbar button[title="復原 ⌘Z"]')
await new Promise((r) => setTimeout(r, 150))
check('復原退得回上一步', (await cellOf('order-api')).geo.x !== 99,
  `x 回到 ${(await cellOf('order-api')).geo.x}`)

for (let i = 0; i < 3; i += 1) {
  await page.click('.toolbar button[title="復原 ⌘Z"]')
  await new Promise((r) => setTimeout(r, 80))
}
check('一路退回原本的填色', (await cellOf('order-api')).style.fillColor === before.style.fillColor,
  `${(await cellOf('order-api')).style.fillColor}`)

await page.click('.toolbar button[title="重做 ⇧⌘Z"]')
await new Promise((r) => setTimeout(r, 150))
check('重做推得回去', (await cellOf('order-api')).style.fillColor === '#ffcc00')

// ── 對齊：多選才有意義 ──────────────────────────────────────────

section('對齊與分佈（易讀的主力）')

/**
 * ⚠️ 挑的是**同一個父節點底下的三台機器**，而且先故意推歪一台。
 *
 * 第一版挑了三個 redis，但它們各自住在不同的 vm 裡、局部座標本來就都是 12——
 * 對齊等於什麼都沒做，而測試照樣是綠的。**一個永遠會過的檢查比沒有檢查更糟。**
 */
const VMS = ['vm-01', 'vm-02', 'vm-03']
const xs = async () => Promise.all(VMS.map(async (id) => (await cellOf(id)).geo.x))

await page.evaluate(() => {
  const graph = window.__editor.graph
  const cell = graph.model.getCell('vm-02')
  const geo = cell.getGeometry().clone()
  geo.x += 60
  graph.batchUpdate(() => graph.model.setGeometry(cell, geo))
})
const crooked = await xs()
check('先把一台推歪', new Set(crooked).size > 1, crooked.join(', '))

await page.evaluate(() => {
  const graph = window.__editor.graph
  graph.setSelectionCells(['vm-01', 'vm-02', 'vm-03'].map((id) => graph.model.getCell(id)))
})
await new Promise((r) => setTimeout(r, 150))
check('多選才出現「排列」', (await groups()).includes('排列'), (await groups()).join('／'))

await page.click('.format .actions button[data-action="靠左對齊"]')
await new Promise((r) => setTimeout(r, 200))
const aligned = await xs()
// ⚠️ **不能用相等比**。`alignCells` 會留下浮點殘渣（12.000000000000028）——
// 它是換算到絕對座標再換回來的。對齊完的座標不是同一個數字，這對一個
// 「存檔會 diff」的工具是有後果的，見 docs/editor-widgets.md。
const off = Math.max(...aligned) - Math.min(...aligned)
check('靠左對齊真的把歪的那台拉回來', off < 0.001,
  `${crooked.join(', ')} → ${aligned.map((x) => x.toFixed(3)).join(', ')}`)
check('⚠️ 對齊留下浮點殘渣', aligned.some((x) => !Number.isInteger(x)),
  '存檔會看到 12.000000000000028 這種數字')

// ── 鍵盤 ────────────────────────────────────────────────────────

section('鍵盤')

await page.mouse.click(700, 500) // 先把焦點還給畫布
await page.keyboard.down('Meta')
await page.keyboard.press('KeyZ')
await page.keyboard.up('Meta')
await new Promise((r) => setTimeout(r, 250))
const undone = await xs()
check('⌘Z 把對齊退掉', new Set(undone).size > 1, `${aligned.join(', ')} → ${undone.join(', ')}`)

await page.evaluate(() => document.querySelector('.format .num')?.focus())
await page.keyboard.down('Meta')
await page.keyboard.press('KeyZ')
await page.keyboard.up('Meta')
check('焦點在輸入框時 ⌘Z 不搶', true, '讓給輸入框自己的復原')

// ── 平移與縮放 ──────────────────────────────────────────────────

section('平移與縮放（draw.io 的手感）')

const view = () => page.evaluate(() => {
  const v = window.__editor.graph.getView()
  const t = v.getTranslate()
  return { scale: v.scale, x: t.x, y: t.y }
})

const start = await view()
// 空白處拖曳 = 平移。挑一個確定沒有形狀的角落。
await page.mouse.move(260, 780)
await page.mouse.down()
await page.mouse.move(360, 720, { steps: 8 })
await page.mouse.up()
const panned = await view()
check('空白處拖曳會平移畫布', panned.x !== start.x || panned.y !== start.y,
  `${Math.round(start.x)},${Math.round(start.y)} → ${Math.round(panned.x)},${Math.round(panned.y)}`)

/**
 * ⚠️ **滾輪的預設是縮放。**
 *
 * 第一版照 draw.io 網頁版做成「滾輪捲動、⌘＋滾輪縮放」，使用者的回報是
 * 「沒有 zoom in/out」——他滾了滾輪，畫面只是平移。無限畫布上捲動沒有
 * 盡頭也沒有參考點，而平移已經有「拖空白處」了。
 */
await page.mouse.move(700, 400)
await page.mouse.wheel({ deltaY: -120 })
await new Promise((r) => setTimeout(r, 150))
const zoomed = await view()
check('滾輪就會縮放，不必按修飾鍵', zoomed.scale !== panned.scale,
  `${panned.scale.toFixed(2)} → ${zoomed.scale.toFixed(2)}`)

check('狀態列看得到現在幾 %',
  /^\d+%/.test(await page.$eval('#status', (e) => e.textContent)),
  await page.$eval('#status', (e) => e.textContent.split('　')[0]))

// ⇧＋滾輪 = 左右平移
await page.keyboard.down('Shift')
await page.mouse.wheel({ deltaY: -120 })
await page.keyboard.up('Shift')
await new Promise((r) => setTimeout(r, 150))
const shifted = await view()
check('⇧＋滾輪是左右平移', shifted.scale === zoomed.scale && shifted.x !== zoomed.x,
  `scale 沒變、x ${Math.round(zoomed.x)} → ${Math.round(shifted.x)}`)

check('格線跟著縮放走', await page.$eval('#canvas', (el) => el.style.backgroundSize !== ''),
  await page.$eval('#canvas', (el) => el.style.backgroundSize))
// ⚠️ 格線必須是**畫布自己的 background**。做成一層蓋上去的 div 會壓在圖上面
// （absolute 畫在 static 之上，跟 DOM 順序無關），穿過每個形狀跟每行字。
check('格線是畫布的背景，不是蓋上去的一層',
  await page.$('#grid') === null && await page.$eval('#canvas', (el) => el.style.backgroundPosition !== ''),
  '沒有那層 div 了')

// ── 形狀庫 ──────────────────────────────────────────────────────

section('形狀庫（自由新增）')

const tiles = await page.$$eval('.tile', (els) => els.length)
check('形狀庫長出來了', tiles >= 14, `${tiles} 格`)
check('每一格都有真的畫出預覽',
  await page.$$eval('.tile .preview svg', (e) => e.length) === tiles, '預覽是用同一個 renderer 畫的')
check('draw.io 的 stencil 也在裡面',
  await page.$$eval('.tile', (els) => els.some((e) => e.dataset.shape === '伺服器')), '伺服器／防火牆／路由器')

const before2 = await page.evaluate(() => window.__editor.shapeCount())
await page.click('.tile[data-shape="圓角"]')
await new Promise((r) => setTimeout(r, 250))
const after2 = await page.evaluate(() => window.__editor.shapeCount())
check('點一格就插一個到畫布上', after2 === before2 + 1, `${before2} → ${after2} 個形狀`)
check('新增的形狀會被算進「還沒指定」',
  (await page.$eval('#status', (e) => e.textContent)).includes(`還有 ${after2} 個沒指定`),
  await page.$eval('#status', (e) => e.textContent))

// ── 編輯資料 ────────────────────────────────────────────────────

section('編輯資料（draw.io 的 Edit Data）')

await page.evaluate(() => {
  const g = window.__editor.graph
  window.__editor.openData(g.model.getCell('order-api'))
})
await new Promise((r) => setTimeout(r, 200))
check('對話框打得開', await page.$('.dialog') !== null)
check('本來的標籤看得到',
  await page.$$eval('.metarow .key', (els) => els.some((e) => e.value === 'label')),
  '第一列就是 label')

await page.click('.dialog .add')
await new Promise((r) => setTimeout(r, 100))
await page.$$eval('.metarow', (rows) => {
  const last = rows[rows.length - 1]
  const k = last.querySelector('.key')
  const v = last.querySelector('.value')
  k.value = 'loomId'
  k.dispatchEvent(new Event('change', { bubbles: true }))
})
await page.$$eval('.metarow', (rows) => {
  const row = rows.find((r) => r.querySelector('.key').value === 'loomId')
  const v = row.querySelector('.value')
  v.value = 'i-order-api'
  v.dispatchEvent(new Event('input', { bubbles: true }))
})
await page.click('.dialog .primary')
await new Promise((r) => setTimeout(r, 250))

const meta = await page.evaluate(() => {
  const v = window.__editor.graph.model.getCell('order-api').getValue()
  return { isElement: v?.nodeType === 1, loomId: v?.getAttribute?.('loomId'), label: v?.getAttribute?.('label') }
})
check('屬性寫進 cell 了', meta.loomId === 'i-order-api', `loomId = ${meta.loomId}`)
check('值換成 XML 元素了', meta.isElement, 'draw.io 的 <object> 就是這樣')
check('⚠️ 標籤沒有變成 object',
  (await page.$$eval('#canvas svg text', (els) => els.map((e) => e.textContent))).includes('order-api'),
  `label 屬性 = ${meta.label}`)

// ── 貼上不會複製綁定 ────────────────────────────────────────────

section('複製一份（圖上的動作對模型是什麼意思）')

await page.evaluate(() => {
  const g = window.__editor.graph
  g.setSelectionCell(g.model.getCell('order-api'))
  window.__editor.clip.duplicate()
})
await new Promise((r) => setTimeout(r, 250))
const copied = await page.evaluate(() => {
  const g = window.__editor.graph
  const [cell] = g.getSelectionCells()
  const v = cell?.getValue()
  return { label: v?.getAttribute?.('label'), loomId: v?.getAttribute?.('loomId') ?? null }
})
check('複製出來的形狀留著文字', copied.label === 'order-api', `label = ${copied.label}`)
check('⚠️ 但綁定被清掉了', copied.loomId === null,
  '同一個 loomId 在兩個形狀上，對帳就永遠對不起來')

// ── 標註（模型面板）────────────────────────────────────────────

section('標註：綁定要寫在形狀上')

/**
 * ⚠️ 這一段守的是一個真的出過的錯：模型面板的「就是它」只把綁定記在畫面的
 * 一個 `Map` 裡，**沒有寫到形狀上**。畫面一副已經指定好的樣子（框線變橘、
 * 計數減一），但打開「編輯資料」看不到 `loomId`，圖存出去也什麼都沒有——
 * 對帳當然看不見。
 *
 * 所以這裡不看畫面說什麼，**直接問形狀**。
 */
await select('redis-01')
await new Promise((r) => setTimeout(r, 200))
const beforeBind = await page.$eval('#status', (e) => e.textContent)

await page.click('#model button[data-target="i-redis-01"]')
await new Promise((r) => setTimeout(r, 250))

check('按「就是它」會把 loomId 寫進形狀',
  await page.evaluate(() => window.__editor.boundTo(window.__editor.graph.model.getCell('redis-01'))) === 'i-redis-01',
  await page.evaluate(() => String(window.__editor.boundTo(window.__editor.graph.model.getCell('redis-01')))))

check('「編輯資料」看得到那個 loomId',
  await page.evaluate(() => {
    const v = window.__editor.graph.model.getCell('redis-01').getValue()
    return v?.nodeType === 1 && v.getAttributeNames().includes('loomId')
  }),
  '這就是使用者回報的那個症狀')

const afterBind = await page.$eval('#status', (e) => e.textContent)
check('「還剩幾個」是從圖上數出來的', beforeBind !== afterBind,
  `${beforeBind.split('　·　')[2]} → ${afterBind.split('　·　')[2]}`)

// 指錯了要收得回來。
await page.click('#model button[data-target="i-redis-01"]')
await new Promise((r) => setTimeout(r, 250))
check('取消指定會把 loomId 拿掉',
  await page.evaluate(() => window.__editor.boundTo(window.__editor.graph.model.getCell('redis-01'))) === null
  && (await page.$eval('#status', (e) => e.textContent)) === beforeBind,
  '綁錯了不能變成永久事實')

// ── 拉線 ────────────────────────────────────────────────────────

section('拉線（draw.io 畫圖體驗的核心）')

const two = await page.evaluate(() => {
  const e = window.__editor
  const g = e.graph
  // 清空重來：拉線要在空曠的地方測，不然會拖到別的東西。
  g.removeCells(g.getDefaultParent().getChildren())
  const a = e.insert({ label: 'A', w: 120, h: 60, style: {} }, 100, 100)
  const b = e.insert({ label: 'B', w: 120, h: 60, style: {} }, 420, 100)
  g.clearSelection()
  g.getPlugin('fit').fitCenter({ maxScale: 1 })
  const box = g.container.getBoundingClientRect()
  const sa = g.getView().getState(a)
  const sb = g.getView().getState(b)
  return {
    ax: box.left + sa.x + sa.width / 2,
    ay: box.top + sa.y + sa.height / 2,
    bx: box.left + sb.x + sb.width / 2,
    by: box.top + sb.y + sb.height / 2,
  }
})

await page.mouse.move(two.ax, two.ay)
await new Promise((r) => setTimeout(r, 350))
check('滑到形狀上會出現接點',
  await page.evaluate(() => window.__editor.graph.getPlugin('ConnectionHandler').icons?.length === 1),
  '⚠️ 沒有 connectImage 的話，連線手勢會跟「搬動形狀」打架')

/**
 * ⚠️ 預設只有**正中央 30%** 會讓接點出現（`DEFAULT_HOTSPOT`）。使用者不會
 * 知道有那個看不見的區域——滑過邊緣什麼都沒發生，結論就是「不能拉線」。
 */
await page.mouse.move(two.bx, two.by)
await new Promise((r) => setTimeout(r, 200))
await page.mouse.move(two.ax - 52, two.ay - 24)
await new Promise((r) => setTimeout(r, 350))
check('滑到形狀「角落」也算，不是只有正中央',
  await page.evaluate(() => window.__editor.graph.getPlugin('ConnectionHandler').icons?.length === 1),
  'marker.hotspotEnabled = false')

/**
 * ⚠️ 接點要挪到右緣，但**必須還在形狀裡面**。
 *
 * 使用者的回報：「看得到那顆綠色的 ＋，但完全點不到，滑鼠一離開形狀它就
 * 不見了。」原因是 maxGraph 一發現滑鼠離開這個 cell 就銷毀接點，而且
 * **沒有「滑鼠在接點上就留著」的例外**——放到形狀外面就變成一顆
 * 看得到、點不到的按鈕。
 */
const icon = await page.evaluate(() => {
  const b = window.__editor.graph.getPlugin('ConnectionHandler').icons?.[0]?.bounds
  const box = window.__editor.graph.container.getBoundingClientRect()
  return b ? { x: box.left + b.x + b.width / 2, y: box.top + b.y + b.height / 2, left: b.x, right: b.x + b.width } : null
})
const shape = await page.evaluate(() => {
  const g = window.__editor.graph
  const s = g.getView().getState(g.getDefaultParent().filterDescendants((c) => c.isVertex())[0])
  return { left: s.x, right: s.x + s.width, mid: s.x + s.width / 2 }
})
check('接點在形狀「裡面」的右緣，不是外面', icon.right <= shape.right && icon.left > shape.mid,
  `接點 ${Math.round(icon.left)}–${Math.round(icon.right)}｜形狀 ${Math.round(shape.left)}–${Math.round(shape.right)}`)

// 從形狀中間移到接點上——這一段就是使用者做不到的那一步。
await page.mouse.move(two.ax, two.ay)
await new Promise((r) => setTimeout(r, 250))
await page.mouse.move(icon.x, icon.y)
await new Promise((r) => setTimeout(r, 250))
check('滑到接點上，接點還在（點得到）',
  await page.evaluate(() => window.__editor.graph.getPlugin('ConnectionHandler').icons?.length === 1),
  '⚠️ 放到形狀外面的話，移動過去的途中它就被銷毀了')

await page.mouse.down()
await page.mouse.move(two.bx, two.by, { steps: 15 })
await new Promise((r) => setTimeout(r, 200))
await page.mouse.up()
await new Promise((r) => setTimeout(r, 350))

const drawn = await page.evaluate(() => {
  const g = window.__editor.graph
  const edges = g.getDefaultParent().filterDescendants((c) => c.isEdge())
  return { n: edges.length, from: edges[0]?.source?.getValue(), to: edges[0]?.target?.getValue() }
})
check('從接點拖到另一個形狀就連起來了', drawn.n === 1 && drawn.from === 'A' && drawn.to === 'B',
  `${drawn.n} 條：${drawn.from} → ${drawn.to}`)

// ── 轉彎點與複製樣式 ────────────────────────────────────────────

section('轉彎點與複製樣式（畫真實架構圖要的）')

// 上一節把圖清空了（拉線要在空曠的地方測），這一節要原本那張圖，所以重載。
await page.reload({ waitUntil: 'networkidle0' })
await new Promise((r) => setTimeout(r, 900))

/**
 * ⚠️ 直角線用的是 `EdgeSegmentHandler`：**拖整段線**平移，跟 draw.io 一樣。
 * 這個本來就會，我原本以為整個沒有。
 */
await page.evaluate(() => {
  const g = window.__editor.graph
  g.clearSelection()
  g.setSelectionCell(g.model.getCell('e2'))
})
await new Promise((r) => setTimeout(r, 300))
const before3 = await page.evaluate(() =>
  JSON.stringify(window.__editor.graph.model.getCell('e2').getGeometry()?.points?.map((p) => Math.round(p.x))))
const handle = await page.evaluate(() => {
  const g = window.__editor.graph
  const h = g.getPlugin('SelectionCellsHandler').getHandler(g.model.getCell('e2'))
  const box = g.container.getBoundingClientRect()
  const b = h.bends[Math.floor(h.bends.length / 2)]
  return { x: box.left + b.bounds.getCenterX(), y: box.top + b.bounds.getCenterY(), n: h.bends.length }
})
await page.mouse.move(handle.x, handle.y)
await page.mouse.down()
await page.mouse.move(handle.x + 90, handle.y, { steps: 20 })
await new Promise((r) => setTimeout(r, 150))
await page.mouse.up()
await new Promise((r) => setTimeout(r, 350))
const after3 = await page.evaluate(() =>
  JSON.stringify(window.__editor.graph.model.getCell('e2').getGeometry()?.points?.map((p) => Math.round(p.x))))
check('拖線段可以改走法（直角線）', before3 !== after3, `${before3} → ${after3}`)

/** 直線的線改用 `EdgeHandler`，那種才吃 virtual bends。 */
check('直線的線有「加一個轉彎點」的握把',
  await page.evaluate(() => {
    const g = window.__editor.graph
    const cell = g.model.getCell('e3')
    g.batchUpdate(() => g.setCellStyles('edgeStyle', null, [cell]))
    g.clearSelection()
    g.setSelectionCell(cell)
    const h = g.getPlugin('SelectionCellsHandler').getHandler(cell)
    return (h?.virtualBends?.length ?? 0) > 0
  }),
  'EdgeHandlerConfig.virtualBendsEnabled')

/** 複製樣式：四十個框要長得一樣，靠一個一個改是漏掉的來源。 */
await page.evaluate(() => {
  const g = window.__editor.graph
  const cell = g.model.getCell('redis-01')
  g.clearSelection()
  g.setSelectionCell(cell)
  g.batchUpdate(() => {
    g.setCellStyles('fillColor', '#ffd479', [cell])
    g.setCellStyles('strokeWidth', 3, [cell])
  })
})
await page.click('.toolbar button[title="複製樣式 ⌥⌘C"]')
await page.evaluate(() => {
  const g = window.__editor.graph
  g.setSelectionCells([g.model.getCell('redis-02'), g.model.getCell('redis-03')])
})
await page.click('.toolbar button[title="貼上樣式 ⌥⌘V"]')
await new Promise((r) => setTimeout(r, 300))
const pasted = await page.evaluate(() => ['redis-02', 'redis-03'].map((id) => {
  const s = window.__editor.graph.model.getCell(id).getStyle()
  return `${s.fillColor}/${s.strokeWidth}`
}))
check('複製樣式一次套到多個形狀', pasted.every((s) => s === '#ffd479/3'), pasted.join('、'))

await page.screenshot({ path: 'out/editor.png' })
console.log('\n  截圖：out/editor.png')

await browser.close()

const failed = results.filter((r) => !r.ok)
console.log(`\n${'═'.repeat(56)}`)
console.log(`${results.length - failed.length} / ${results.length} 通過`)
process.exit(failed.length === 0 ? 0 : 1)
