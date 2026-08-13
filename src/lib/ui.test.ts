/**
 * 介面縮放。
 *
 * 這是無障礙功能，所以守的是「**它真的會生效、而且下次還在**」——
 * 一個記不住的設定等於每次開 App 都要重新調一次，那沒人會用。
 */

import { beforeEach, describe, expect, it } from 'vitest'
import { SCALES, apply, load } from './ui'

function zoom(): string {
  return document.documentElement.style.getPropertyValue('--zoom')
}

describe('介面縮放', () => {
  beforeEach(() => {
    localStorage.clear()
    document.documentElement.style.removeProperty('--zoom')
  })

  it('沒設定過時是最小的那一級', () => {
    // 加這個功能不該讓已經習慣現狀的人被迫改變。
    expect(load()).toBe('small')
  })

  it('最小的那一級就是原本的大小', () => {
    // 「目前的字體大小就是最小的那一個」——所以它的縮放必須剛好是 1。
    expect(SCALES.find((s) => s.value === 'small')?.zoom).toBe(1)
  })

  it('三級是遞增的', () => {
    const zooms = SCALES.map((s) => s.zoom)
    expect(zooms).toEqual([...zooms].sort((a, b) => a - b))
    expect(new Set(zooms).size).toBe(3)
  })

  it('套用會寫進 CSS 變數', () => {
    apply('large')
    expect(zoom()).toBe('1.32')
  })

  it('下次開起來是同一個大小', () => {
    apply('medium')
    expect(load()).toBe('medium')
  })

  it('存了怪東西進去時退回最小，不是壞掉', () => {
    // localStorage 是使用者機器上的東西，任何人都可能弄髒它。
    localStorage.setItem('diagram-loom.ui-scale', '巨大')
    expect(load()).toBe('small')
  })
})
