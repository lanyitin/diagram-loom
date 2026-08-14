/**
 * App 外殼（`index.html`）該有的東西。
 *
 * # 為什麼「作業系統別改我打的字」值得一條測試
 *
 * 這個工具裡打的幾乎都是**會被逐字比對**的字串：slug、IP、JDBC URL、
 * 萬用字元樣式。macOS 預設開著自動大寫，於是在空白欄位打 `redis-01`
 * 會存成 `Redis-01`——那是一個不同的名字，而使用者只會看到連線接不起來。
 *
 * 關掉的方式是把屬性掛在 `<html>` 上，靠**繼承**蓋住全部的輸入框。
 * 這件事的特性是「安靜地生效、也安靜地失效」：屬性被誰刪掉之後畫面
 * 完全一樣，要等到某個人的名字被偷偷改掉才會發現。
 */

import { describe, expect, it } from 'vitest'
import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

const shell = readFileSync(join(import.meta.dirname, '..', 'index.html'), 'utf8')
const components = join(import.meta.dirname, 'components')

describe('作業系統不准改寫使用者打的字', () => {
  it('關掉自動大寫與自動修正', () => {
    expect(shell).toMatch(/<html[^>]*\bautocorrect="off"/)
    expect(shell).toMatch(/<html[^>]*\bautocapitalize="off"/)
  })

  it('沒有任何元件把它開回來', () => {
    // 這兩個屬性會繼承，所以某個輸入框自己寫 `autocorrect="on"` 就能
    // 在那一格把它開回去，而其他四十幾格看起來一切正常。
    const offenders = readdirSync(components)
      .filter((f) => f.endsWith('.vue'))
      .filter((f) =>
        /\bautocorrect=|\bautocapitalize=/.test(
          readFileSync(join(components, f), 'utf8'),
        ),
      )

    expect(offenders).toEqual([])
  })
})

describe('替代符號要另外一層擋', () => {
  it('Rust 那邊有關掉系統層的自動替換', () => {
    // 智慧型引號、破折號、文字替換是 NSSpellChecker 的設定，
    // 網頁那一層碰不到——只有 index.html 的話，`"` 還是會被換成 `""`。
    const rust = readFileSync(
      join(import.meta.dirname, '..', 'src-tauri', 'src', 'lib.rs'),
      'utf8',
    )

    expect(rust).toContain('stop_the_os_from_rewriting_what_you_type()')
    for (const key of [
      'NSAutomaticQuoteSubstitutionEnabled',
      'NSAutomaticDashSubstitutionEnabled',
      'NSAutomaticTextReplacementEnabled',
      'NSAutomaticSpellingCorrectionEnabled',
      'NSAutomaticCapitalizationEnabled',
      'NSAutomaticPeriodSubstitutionEnabled',
    ]) {
      expect(rust).toContain(key)
    }
  })
})
