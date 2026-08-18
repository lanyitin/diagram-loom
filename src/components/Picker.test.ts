/**
 * 會打字的下拉選單。
 *
 * 這裡真正在守的是一條：**失焦時絕不安靜地清空**。打了一半沒選中就走掉，
 * 要退回原本選的那個——悄悄變空是這個工具最不能出的那種錯，
 * 使用者以為填好了，其實沒有。
 */

import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import Picker from './Picker.vue'

const OPTIONS = [
  { value: 'a', label: 'redis-01', group: '服務', hint: '10.0.1.11' },
  { value: 'b', label: 'redis-02', group: '服務', hint: '10.0.1.12' },
  { value: 'c', label: 'f5-01', group: '設備' },
]

function picker(modelValue: string | null = null, extra = {}) {
  return mount(Picker, { props: { modelValue, options: OPTIONS, ...extra } })
}

const items = (w: ReturnType<typeof picker>) =>
  w.findAll('.item').map((b) => b.find('.label').text() || b.text())

describe('打開', () => {
  it('沒展開時看不到清單', () => {
    expect(picker().find('.menu').exists()).toBe(false)
  })

  it('點進去就展開，而且看得到全部', async () => {
    const w = picker()
    await w.find('input').trigger('focus')
    expect(items(w)).toEqual(['redis-01', 'redis-02', 'f5-01'])
  })

  it('選中的那個顯示在輸入框裡', () => {
    expect(picker('b').find('input').element.value).toBe('redis-02')
  })
})

describe('打字搜尋', () => {
  it('照名字篩', async () => {
    const w = picker()
    await w.find('input').setValue('redis')
    expect(items(w)).toEqual(['redis-01', 'redis-02'])
  })

  it('副標也算——使用者手上常常只有 IP', async () => {
    const w = picker()
    await w.find('input').setValue('10.0.1.12')
    expect(items(w)).toEqual(['redis-02'])
  })

  it('不分大小寫', async () => {
    const w = picker()
    await w.find('input').setValue('F5')
    expect(items(w)).toEqual(['f5-01'])
  })

  it('找不到就直說，不是給一片空白', async () => {
    // 空的清單跟「壞掉了」長得一樣。
    const w = picker()
    await w.find('input').setValue('沒有這種東西')
    expect(w.find('.none').text()).toContain('沒有這種東西')
  })
})

describe('選取', () => {
  it('點一下就選中', async () => {
    const w = picker()
    await w.find('input').trigger('focus')
    await w.findAll('.item')[1]!.trigger('click')
    expect(w.emitted('update:modelValue')).toEqual([['b']])
  })

  it('選完就收起來', async () => {
    const w = picker()
    await w.find('input').trigger('focus')
    await w.findAll('.item')[0]!.trigger('click')
    expect(w.find('.menu').exists()).toBe(false)
  })

  it('允許不選時才有「不指定」那一項', async () => {
    const w = picker(null, { allowEmpty: true })
    await w.find('input').trigger('focus')
    expect(w.find('.item.empty').exists()).toBe(true)

    const strict = picker()
    await strict.find('input').trigger('focus')
    expect(strict.find('.item.empty').exists()).toBe(false)
  })
})

describe('鍵盤', () => {
  it('上下鍵移動，Enter 選中', async () => {
    const w = picker()
    const input = w.find('input')
    await input.trigger('focus')
    await input.trigger('keydown', { key: 'ArrowDown' })
    await input.trigger('keydown', { key: 'Enter' })
    expect(w.emitted('update:modelValue')).toEqual([['b']])
  })

  it('走到底不會掉出去', async () => {
    const w = picker()
    const input = w.find('input')
    await input.trigger('focus')
    for (let i = 0; i < 10; i++) await input.trigger('keydown', { key: 'ArrowDown' })
    await input.trigger('keydown', { key: 'Enter' })
    expect(w.emitted('update:modelValue')).toEqual([['c']])
  })

  it('打字之後 Enter 選到的是第一個相符的，不是上一輪停的位置', async () => {
    const w = picker()
    const input = w.find('input')
    await input.trigger('focus')
    await input.trigger('keydown', { key: 'ArrowDown' })
    await input.setValue('f5')
    await input.trigger('keydown', { key: 'Enter' })
    expect(w.emitted('update:modelValue')).toEqual([['c']])
  })

  it('Esc 收起來，而且不改值', async () => {
    const w = picker('a')
    const input = w.find('input')
    await input.trigger('focus')
    await input.setValue('redis-02')
    await input.trigger('keydown', { key: 'Escape' })

    expect(w.find('.menu').exists()).toBe(false)
    expect(w.emitted('update:modelValue')).toBeUndefined()
    expect(input.element.value).toBe('redis-01')
  })
})

describe('不安靜地清空', () => {
  it('打了一半就走掉，退回原本選的那個', async () => {
    // 這是整個元件最重要的一條。悄悄變空的話，使用者以為填好了，其實沒有。
    const w = picker('a')
    const input = w.find('input')
    await input.trigger('focus')
    await input.setValue('打到一半')
    await input.trigger('blur')

    expect(w.emitted('update:modelValue')).toBeUndefined()
    expect(input.element.value).toBe('redis-01')
  })

  it('本來就沒選的話，走掉之後還是沒選', async () => {
    const w = picker(null)
    const input = w.find('input')
    await input.trigger('focus')
    await input.setValue('亂打')
    await input.trigger('blur')

    expect(w.emitted('update:modelValue')).toBeUndefined()
    expect(input.element.value).toBe('')
  })
})

describe('分組', () => {
  it('有分組就畫標題', async () => {
    const w = picker()
    await w.find('input').trigger('focus')
    expect(w.findAll('.group').map((g) => g.text())).toEqual(['服務', '設備'])
  })

  it('沒分組就不畫標題', async () => {
    const w = mount(Picker, {
      props: { modelValue: null, options: [{ value: 'a', label: 'x' }] },
    })
    await w.find('input').trigger('focus')
    expect(w.find('.group').exists()).toBe(false)
  })
})

describe('停用', () => {
  it('停用時點不開', async () => {
    const w = picker(null, { disabled: true })
    await w.find('input').trigger('focus')
    expect(w.find('.menu').exists()).toBe(false)
  })
})

describe('選單釘在哪', () => {
  /**
   * 假裝視窗多大、輸入框在哪。happy-dom 不做版面，所以兩件事都要自己講——
   * 不講的話幾個案例會走進同一個分支，然後全部都綠。
   */
  function place(
    w: ReturnType<typeof picker>,
    box: { top: number; left?: number; width?: number; height?: number },
    viewport: { w?: number; h: number },
  ) {
    Object.defineProperty(window, 'innerHeight', { value: viewport.h, configurable: true })
    Object.defineProperty(window, 'innerWidth', { value: viewport.w ?? 1024, configurable: true })
    const { top, left = 100, width = 160, height = 24 } = box
    w.find('input').element.getBoundingClientRect = () =>
      ({ top, bottom: top + height, left, right: left + width, width, height }) as DOMRect
  }

  async function openAt(
    box: { top: number; left?: number; width?: number },
    viewport: { w?: number; h: number },
  ) {
    const w = picker()
    place(w, box, viewport)
    await w.find('input').trigger('focus')
    await w.vm.$nextTick()
    return w.find('.menu').attributes('style') ?? ''
  }

  it('是 fixed，不是 absolute', () => {
    // **這是整組最重要的一條。** 絕對定位的選單會被祖先的 `overflow` 切掉：
    // `ResourceForm` 的對話框與 `DiagramBinder` 的清單都是 `overflow: auto`，
    // 而在標註面板裡它看得見的高度實測是 0——使用者看到的是一個打了字
    // 卻沒有任何選項的框，他會以為「沒有符合的」然後放棄。
    //
    // 讀原始碼而不是 `getComputedStyle`：happy-dom 不套 scoped style。
    // 同一個作法見 `ResourceForm.test.ts` 守 `<select>` 那幾條。
    const source = readFileSync(join(import.meta.dirname, 'Picker.vue'), 'utf8')
    const rule = source.match(/\.menu \{([^}]*)\}/)
    expect(rule, '找不到 .menu 那條規則').not.toBeNull()
    expect(rule![1]).toContain('position: fixed')
    expect(rule![1]).not.toContain('absolute')
  })

  it('下面放得下就開在輸入框下面', async () => {
    const style = await openAt({ top: 40 }, { h: 700 })
    expect(style).toContain('top: 66px')   // 40 + 24 + 2
    expect(style).not.toContain('bottom:')
  })

  it('下面塞不下、上面塞得下就往上開', async () => {
    // 不然使用者只看得到半個項目，而且捲不到——選單不佔位置。
    const style = await openAt({ top: 640 }, { h: 700 })
    expect(style).toContain('bottom: 62px')   // 700 - 640 + 2
    expect(style).not.toContain('top:')
  })

  it('兩邊都不夠就還是往下——那時候至少捲得到', async () => {
    // 上面 130、下面 146，都不到 260。往上開只會把問題換一邊。
    const style = await openAt({ top: 130 }, { h: 300 })
    expect(style).toContain('top: 156px')
  })

  it('窄格子裡也要有最小寬度', async () => {
    // 跟著格子一樣窄的話，選項會一個字一行往下排——看起來像元件壞了。
    const style = await openAt({ top: 40, width: 80 }, { h: 700 })
    expect(style).toContain('min-width: 220px')
  })

  it('欄位比下限寬時就用欄位的寬度', async () => {
    const style = await openAt({ top: 40, width: 400 }, { h: 700 })
    expect(style).toContain('min-width: 400px')
  })

  it('靠右邊的格子往左推，不會被推出畫面', async () => {
    // 視窗 1024、選單至少 220，所以最右只能到 1024 - 220 - 8 = 796。
    const style = await openAt({ top: 40, left: 950, width: 60 }, { w: 1024, h: 700 })
    expect(style).toContain('left: 796px')
  })

  it('靠左邊也不會貼到邊緣', async () => {
    const style = await openAt({ top: 40, left: 0, width: 60 }, { w: 1024, h: 700 })
    expect(style).toContain('left: 8px')
  })
})
