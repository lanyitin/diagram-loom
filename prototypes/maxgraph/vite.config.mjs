import { createReadStream, existsSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'

/**
 * `vendor/drawio/stencils/` 在這個資料夾外面、而且不進版控，所以拿一小段
 * middleware 把它接到 `/stencils/` 底下。原型不該為了一個測試素材複製 152 MB。
 *
 * ⚠️ 用 `resolve.alias` 是錯的——那條管的是 **import 的模組解析**，不是網址。
 * 設了之後 `fetch('/stencils/arrows.xml')` 會落到 SPA fallback，拿回一份
 * `index.html`，然後 `res.ok` 是 true、XML 解析出 0 個形狀。
 * **它不會報錯，只會安靜地少畫東西**——這一輪就被它騙過一次。
 */
const STENCILS = fileURLToPath(new URL('../../vendor/drawio/stencils/', import.meta.url))

function stencils() {
  return {
    name: 'drawio-stencils',
    configureServer(server) {
      server.middlewares.use('/stencils', (req, res, next) => {
        // ⚠️ 這是「把使用者給的路徑轉成檔案」的函式，也就是最典型會被
        // `../../../../etc/passwd` 打穿的地方。
        //
        // **接起來再看開頭是不是根目錄是不夠的**——`../` 拼進去之後字串
        // 的開頭仍然是根目錄，但它指到別的地方。所以照 `src-tauri/src/drawio.rs`
        // 那條規矩：**正規化之後**才確認還在根目錄底下。
        const asked = decodeURIComponent(req.url.replace(/^\//, '').split('?')[0])
        const file = resolve(STENCILS, asked)
        if (!file.startsWith(STENCILS) || !existsSync(file)) return next()
        res.setHeader('Content-Type', 'text/xml; charset=utf-8')
        createReadStream(file).pipe(res)
      })
    },
  }
}

export default defineConfig({
  plugins: [stencils()],
  server: { port: 5180 },
})
