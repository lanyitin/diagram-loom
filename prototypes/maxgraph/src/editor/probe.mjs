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

await page.screenshot({ path: 'out/editor.png' })
console.log('\n  截圖：out/editor.png')

await browser.close()

const failed = results.filter((r) => !r.ok)
console.log(`\n${'═'.repeat(56)}`)
console.log(`${results.length - failed.length} / ${results.length} 通過`)
process.exit(failed.length === 0 ? 0 : 1)
