/**
 * 版面對縮放的耐受度。
 *
 * # 為什麼要有這個
 *
 * 真的踩過：`#app` 寫成 `height: calc(100% / var(--zoom))`，以為 zoom 會
 * 乘回來。結果視窗底下空出一條黑帶——**百分比不會被 zoom 放大，
 * 絕對長度才會**，所以只有除、沒有乘。
 *
 * 而且它只在「中」與「大」才看得出來，預設的「小」是 `zoom: 1`，
 * 完全正常。這種只在非預設設定下才壞的東西，最容易活很久沒人發現。
 *
 * 這裡守的是**寫法**，不是渲染結果——單元測試環境沒有排版引擎。
 * 所以規則訂得很簡單：碰到高度的地方，一律不准出現會被 zoom 放大的單位。
 */

import { describe, expect, it } from 'vitest'
import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

const COMPONENTS = join(import.meta.dirname, 'components')

/** 註解裡會出現當反例的寫法，那不算。 */
function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, '')
}

function styleBlocks(): { file: string; css: string }[] {
  const files = ['App.vue', ...readdirSync(COMPONENTS).filter((f) => f.endsWith('.vue'))]
  return files.map((f) => {
    const path = f === 'App.vue' ? join(import.meta.dirname, f) : join(COMPONENTS, f)
    const src = readFileSync(path, 'utf8')
    const m = /<style[^>]*>([\s\S]*)<\/style>/.exec(src)
    return { file: f, css: stripComments(m?.[1] ?? '') }
  })
}

describe('版面在縮放之後不會破', () => {
  it('#app 不靠算高度貼滿視窗', () => {
    // `inset: 0` 不管 zoom 怎麼對待長度都對——零乘上任何倍率都是零。
    const css = stripComments(readFileSync(join(import.meta.dirname, 'styles.css'), 'utf8'))
    const app = /(?:^|\n)#app\s*\{([\s\S]*?)\}/.exec(css)?.[1] ?? ''
    expect(app).toContain('zoom: var(--zoom)')
    expect(app).toContain('inset: 0')
    expect(app).not.toMatch(/height:/)
  })

  it('沒有任何元件用 vh', () => {
    // vh 會不會被 zoom 放大是未知的。未知的東西不要放在版面的骨架上——
    // 對話框的 max-height 用 vh 的話，放大之後會蓋滿整個視窗。
    const 用了 = styleBlocks()
      .filter(({ css }) => /\d+vh\b/.test(css))
      .map(({ file }) => file)
    expect(用了).toEqual([])
  })

  it('沒有人在 CSS 裡拿 calc 去算 zoom', () => {
    // `zoom` 是個老屬性，吃不吃 `calc()` 各家不一致，而**解析失敗是靜悄悄的**：
    // 那一行被整條丟掉，畫面照舊壞，看不出是這裡的問題。踩過一次。
    // 倒數在 `ui.ts` 算好成一個純數字（`--unzoom`）。
    const 用了的 = [...styleBlocks(), { file: 'styles.css', css: stripComments(readFileSync(join(import.meta.dirname, 'styles.css'), 'utf8')) }]
      .filter(({ css }) => /zoom:\s*calc\(/.test(css))
      .map(({ file }) => file)
    expect(用了的).toEqual([])
  })

  it('沒有人再拿長度去除 --zoom', () => {
    // 除了不會被乘回來。要嘛用百分比（zoom 不碰），要嘛用 px（zoom 會等比放大）。
    //
    const 除了的 = [...styleBlocks(), { file: 'styles.css', css: stripComments(readFileSync(join(import.meta.dirname, 'styles.css'), 'utf8')) }]
      .filter(({ css }) => /\/\s*var\(--zoom\)/.test(css))
      .map(({ file }) => file)
    expect(除了的).toEqual([])
  })
})

describe('畫布不再是一個 iframe', () => {
  /**
   * 這一條以前守的是相反的東西：draw.io 的 iframe 必須用 `--unzoom` 把介面
   * 縮放抵銷掉，否則它的格式面板會被擠成一條（真的踩過）。
   *
   * 換成自己跑 maxGraph 之後**那個問題整個消失了**——畫布跟我們在同一個
   * 文件裡，`zoom` 對它跟對別的元件一樣。所以改成守反面：不要再有 iframe，
   * 也不要再有為了它而存在的抵銷。
   */
  it('圖不再用 iframe 畫', () => {
    const files = readdirSync(join(import.meta.dirname, 'components'))
      .filter((f) => f.endsWith('.vue'))
      .filter((f) => /<iframe/.test(readFileSync(join(import.meta.dirname, 'components', f), 'utf8')))
    expect(files).toEqual([])
  })

  it('沒有人再需要 --unzoom', () => {
    // 留著的話，下一個讀的人會以為還有東西在對抗介面縮放。
    const 用了的 = styleBlocks()
      .filter(({ css }) => /var\(--unzoom\)/.test(css))
      .map(({ file }) => file)
    expect(用了的).toEqual([])
  })
})
