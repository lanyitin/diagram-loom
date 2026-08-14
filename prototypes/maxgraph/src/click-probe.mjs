/**
 * 探針的第二半：**真的用滑鼠點下去**。
 *
 * `probe.mjs` 量得到座標對不對，量不到「點得到形狀嗎」——而那正是整個
 * 換引擎問題的動機。所以這支開一個真的瀏覽器，點一個方框，看側邊欄有沒有
 * 認出它，然後按「就是它」，看綁定有沒有回到圖上。
 *
 * 用系統上已經裝好的 Chrome（`puppeteer-core`），不另外下載一份瀏覽器。
 *
 * 跑法：另一個終端機開著 `pnpm app`，然後 `node src/click-probe.mjs`。
 * 離開碼：0 全過、1 有項目失敗。
 */

import puppeteer from 'puppeteer-core'

const CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
const URL = process.env.APP ?? 'http://localhost:5180/'

const results = []
const check = (name, ok, detail = '') => {
  results.push({ name, ok, detail })
  console.log(`${ok ? '  ✓' : '  ✗'} ${name}${detail ? `　${detail}` : ''}`)
}

const browser = await puppeteer.launch({
  executablePath: CHROME,
  headless: true,
  args: ['--window-size=1400,900'],
})
const page = await browser.newPage()
await page.setViewport({ width: 1400, height: 900 })

const errors = []
page.on('pageerror', (e) => errors.push(e.message))
page.on('console', (m) => {
  // favicon 的 404 也算 console error。把它算進來的話，這一項永遠是紅的，
  // 而永遠紅的檢查等於沒有檢查——真的錯誤會被當成背景雜訊。
  // ⚠️ 要看 `location().url`，不是訊息文字：那句話只寫「Failed to load
  // resource: 404」，看不出是誰。
  const from = m.location()?.url ?? ''
  if (m.type() === 'error' && !from.includes('favicon')) errors.push(`${m.text()}（${from}）`)
})
page.on('requestfailed', (r) => {
  if (!r.url().includes('favicon')) errors.push(`載不到 ${r.url()}`)
})

await page.goto(URL, { waitUntil: 'networkidle0' })
await new Promise((r) => setTimeout(r, 800))

console.log('\n── 畫得出來嗎 ────────────────────────────────────────────')

check('沒有 JavaScript 錯誤', errors.length === 0, errors[0] ?? '')

const log = await page.$eval('#log', (el) => el.textContent.trim())
console.log(log.split('\n').map((l) => `  · ${l}`).join('\n'))

/** SVG 上真的長出東西了嗎。maxGraph 是畫在 SVG 裡的。 */
const svg = await page.evaluate(() => ({
  groups: document.querySelectorAll('#canvas svg g').length,
  paths: document.querySelectorAll('#canvas svg path').length,
  rects: document.querySelectorAll('#canvas svg rect').length,
  texts: [...document.querySelectorAll('#canvas svg text')].map((t) => t.textContent),
}))
check('畫布上有形狀', svg.rects > 0 || svg.paths > 0, `${svg.rects} 個矩形、${svg.paths} 條路徑`)
check(
  '標籤畫出來了',
  svg.texts.includes('order-api') && svg.texts.includes('site-tpe'),
  `${svg.texts.length} 段文字`,
)

// ── 問題三：點形狀 → 標註它代表誰 ───────────────────────────────

console.log('\n── 點得到形狀嗎（問題三） ───────────────────────────────')

/** 用 SVG 上那段文字的位置去點——這就是使用者眼睛看到的位置。 */
const at = await page.evaluate(() => {
  const text = [...document.querySelectorAll('#canvas svg text')].find(
    (t) => t.textContent === 'order-api',
  )
  if (!text) return null
  const box = text.getBoundingClientRect()
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 }
})
check('找得到 order-api 那個框', Boolean(at), at ? `在 ${Math.round(at.x)},${Math.round(at.y)}` : '')

if (at) {
  await page.mouse.click(at.x, at.y)
  await new Promise((r) => setTimeout(r, 200))

  const hint = await page.$eval('#hint', (el) => el.textContent)
  check('點下去之後知道使用者選了誰', hint.includes('order-api'), hint)

  /** 按「就是它」——綁定要回到圖上，不是只留在側邊欄。 */
  await page.click('button[data-target="i-order-api"]')
  await new Promise((r) => setTimeout(r, 200))

  const after = await page.evaluate(() => ({
    left: document.getElementById('left').textContent,
    labels: [...document.querySelectorAll('#canvas svg text')].map((t) => t.textContent),
    stroke: [...document.querySelectorAll('#canvas svg *')].filter(
      (el) => el.getAttribute('stroke') === '#b4552d',
    ).length,
  }))
  check('指定之後「還剩幾個」跟著少一個', after.left.includes('還剩 10 個'), after.left)
  check('綁定看得到在圖上', after.labels.includes('= order-api'), '形狀的標籤變了')
  check('綁定的形狀畫成不一樣的顏色', after.stroke > 0, `${after.stroke} 個元素換了框線色`)
}

// ── 問題四的最後一哩：畫得出來 ──────────────────────────────────

console.log('\n── draw.io 的形狀畫得出來嗎（問題四） ───────────────────')

const stencil = await page.evaluate(() => ({
  ...window.__probe,
  labelled: [...document.querySelectorAll('#canvas svg text')].some(
    (t) => t.textContent === 'draw.io 的形狀',
  ),
  paths: document.querySelectorAll('#canvas svg path').length,
}))
check('stencil 檔真的載進來了', stencil.stencils > 0, `${stencil.stencils} 個形狀`)
// ⚠️ 只看「標籤畫出來了」會被騙：查不到 stencil 時 maxGraph 會**默默退回
// 矩形**，標籤照樣在。所以要問註冊表查不查得回來。
check('style.shape 查得回那個 stencil', stencil.resolved === true, '查不到就會默默變成矩形')
check('形狀畫成路徑而不是矩形', stencil.labelled && stencil.paths > 0, `${stencil.paths} 條路徑`)

await page.screenshot({ path: 'out/screenshot.png', fullPage: false })
console.log('\n  截圖：out/screenshot.png')

await browser.close()

const failed = results.filter((r) => !r.ok)
console.log(`\n${'═'.repeat(60)}`)
console.log(`${results.length - failed.length} / ${results.length} 通過`)
process.exit(failed.length === 0 ? 0 : 1)
