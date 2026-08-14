/**
 * 從 draw.io 的形狀庫挑一小包進版控。
 *
 * # 為什麼要挑
 *
 * `vendor/drawio/stencils/` 有 **1561 個形狀、42 MB**，而且整包不進版控
 * （見 CLAUDE.md）。畫一張系統架構圖用得到的其實只有幾十個——伺服器、
 * 防火牆、路由器、資料庫那一類。
 *
 * 全部帶著走太重，完全不帶又畫不出真實的架構圖，所以挑一包**進版控**：
 * 換一台機器 clone 下來就有，不必先去弄那 152 MB。
 *
 * # 為什麼保留 `<shapes name>` 的結構
 *
 * draw.io 的形狀名是 `<shapes name>` + `<shape name>` 組出來的
 * （`mxgraph.networks.server`）。保留原本的結構，樣式字串就跟 draw.io 通用——
 * 使用者上傳的圖用到這些形狀時，我們畫得出來。
 *
 * 跑法：`mise run stencils`（需要 vendor/drawio）。產出要一起 commit。
 */

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const VENDOR = join(here, '../vendor/drawio/stencils')
const OUT = join(here, '../src/assets/stencils')

/**
 * 挑選標準：**畫一張部署／系統架構圖真的會用到的**。
 *
 * 刻意不挑的：印表機、遊戲手把、衛星、病毒那些。它們服務的是別種圖，
 * 而形狀庫每多一個就多一次分心（見 `docs/editor-widgets.md`）。
 */
const PICKS = {
  'networks.xml': [
    // 機器與服務
    'Server', 'Virtual Server', 'Web Server', 'Mail Server', 'Proxy Server',
    'Mainframe', 'Supercomputer', 'Virtual PC', 'Desktop PC', 'Laptop', 'Terminal',
    // 網路設備
    'Firewall', 'Router', 'Switch', 'Hub', 'Load Balancer', 'Modem',
    'Wireless Hub', 'Patch Panel', 'Rack',
    // 儲存
    'Storage', 'External Storage', 'NAS Filer', 'Server Storage', 'Tape Storage',
    // 邊界與人
    'Cloud', 'Secured', 'Unsecure', 'Users', 'User Male', 'User Female', 'Mobile', 'Tablet',
  ],
}

mkdirSync(OUT, { recursive: true })

let total = 0
let bytes = 0

for (const [file, wanted] of Object.entries(PICKS)) {
  const xml = readFileSync(join(VENDOR, file), 'utf8')
  const pack = xml.match(/<shapes[^>]*\bname="([^"]+)"/)?.[1]
  if (!pack) throw new Error(`${file} 裡找不到 <shapes name>`)

  // 一個 `<shape ...>…</shape>` 為一段。用正規表示式切是安全的：
  // stencil 的 shape 不會巢狀（那是它的 XSD 保證的）。
  const shapes = [...xml.matchAll(/<shape\b[\s\S]*?<\/shape>/g)].map((m) => m[0])
  const picked = shapes.filter((s) => {
    const name = s.match(/<shape[^>]*\bname="([^"]+)"/)?.[1]
    return name && wanted.includes(name)
  })

  const missing = wanted.filter((w) => !picked.some((s) => s.includes(`name="${w}"`)))
  if (missing.length) throw new Error(`${file} 裡找不到：${missing.join('、')}`)

  const out = `<shapes name="${pack}">\n${picked.join('\n')}\n</shapes>\n`
  const target = join(OUT, file)
  writeFileSync(target, out)

  total += picked.length
  bytes += out.length
  console.log(`${file}：挑了 ${picked.length} / ${shapes.length} 個，${(out.length / 1024).toFixed(0)} KB`)
}

console.log(`\n共 ${total} 個形狀，${(bytes / 1024).toFixed(0)} KB → src/assets/stencils/`)
