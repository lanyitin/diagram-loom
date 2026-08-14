/**
 * AI 助手面板。
 *
 * 守的是一件事：**畫面不能讓人以為自己比實際安全。**
 * 關掉 token 檢查之後，那句警告一定要出現——不然使用者會以為
 * 三道鎖都還在。
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import AgentPanel from './AgentPanel.vue'
import { useProject } from '../lib/store'
import { commands } from '../lib/bindings'
import type { McpStatus } from '../lib/model'

vi.mock('../lib/bindings', () => ({
  commands: {
    mcpStatus: vi.fn(),
    startMcp: vi.fn(),
    stopMcp: vi.fn(),
    mcpConfig: vi.fn(),
    setMcpConfig: vi.fn(),
    regenerateMcpToken: vi.fn(),
  },
}))

function state(extra: Partial<McpStatus> = {}): McpStatus {
  return {
    running: true,
    url: 'http://127.0.0.1:53809/mcp',
    preferredPort: 53809,
    requireToken: true,
    autostart: false,
    autostartFailed: null,
    ...extra,
  }
}

describe('AI 助手面板', () => {
  let store: ReturnType<typeof useProject>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useProject()
    store.agentPanelOpen = true
    vi.mocked(commands.mcpStatus).mockResolvedValue({ status: 'ok', data: state() } as never)
    vi.mocked(commands.mcpConfig).mockResolvedValue({ status: 'ok', data: '{"mcpServers":{}}' } as never)
    vi.mocked(commands.setMcpConfig).mockResolvedValue({ status: 'ok', data: state() } as never)
    vi.mocked(commands.regenerateMcpToken).mockResolvedValue({ status: 'ok', data: state() } as never)
  })

  async function open() {
    const w = mount(AgentPanel)
    await flushPromises()
    return w
  }

  it('把上次設定的埠帶回輸入框', async () => {
    // 設定活不過重啟的話，「不用每次重貼」這個功能就沒意義了。
    const w = await open()
    expect((w.find('.field input').element as HTMLInputElement).value).toBe('53809')
  })

  it('留空表示交給系統挑', async () => {
    vi.mocked(commands.mcpStatus).mockResolvedValue({
      status: 'ok', data: state({ preferredPort: null }),
    } as never)
    const w = await open()
    expect((w.find('.field input').element as HTMLInputElement).value).toBe('')
  })

  it('改了埠就送出去', async () => {
    const w = await open()
    await w.find('.field input').setValue('40000')
    await w.find('.field input').trigger('change')
    expect(commands.setMcpConfig).toHaveBeenCalledWith(40000, true)
  })

  it('填了不是埠的東西就當作沒指定，不是送一個壞值下去', async () => {
    const w = await open()
    await w.find('.field input').setValue('七萬')
    await w.find('.field input').trigger('change')
    expect(commands.setMcpConfig).toHaveBeenCalledWith(null, true)
  })

  it('可以換一組 token', async () => {
    const w = await open()
    const button = w.findAll('button').find((b) => b.text().includes('換一組'))
    expect(button, '找不到換 token 的按鈕').toBeTruthy()
    await button!.trigger('click')
    expect(commands.regenerateMcpToken).toHaveBeenCalled()
  })

  it('token 檢查關掉時就沒有換 token 的按鈕', async () => {
    // 沒在檢查的東西還給人換，只是製造「我有做什麼」的錯覺。
    vi.mocked(commands.mcpStatus).mockResolvedValue({
      status: 'ok', data: state({ requireToken: false }),
    } as never)
    const w = await open()
    expect(w.findAll('button').some((b) => b.text().includes('換一組'))).toBe(false)
  })

  it('關掉 token 檢查時，畫面一定要說出少了什麼', async () => {
    // 這是整份測試的重點。看不到警告的人會以為三道鎖都還在。
    vi.mocked(commands.mcpStatus).mockResolvedValue({
      status: 'ok', data: state({ requireToken: false }),
    } as never)
    const w = await open()

    const warn = w.find('.notes .warn')
    expect(warn.exists()).toBe(true)
    expect(warn.text()).toContain('任何')
    expect(warn.text()).toContain('共用')
  })

  it('開著 token 檢查時不出現那句警告', async () => {
    const w = await open()
    expect(w.find('.notes .warn').exists()).toBe(false)
  })

  it('設定填好但沒啟用時直說', async () => {
    // 使用者踩過一次：設定都在、以為就能用，其實忘了打開開關。
    vi.mocked(commands.mcpStatus).mockResolvedValue({
      status: 'ok', data: state({ running: false }),
    } as never)
    const w = await open()

    expect(w.find('.switch').text()).toContain('連不進來')
    expect(w.find('.hint').text()).toContain('還沒啟用')
  })

  it('打開就記住，而且畫面說得出來', async () => {
    // 這裡曾經有兩個控制項：一個開關，加一個「開啟 App 時自動啟用」。
    // 於是打開端點的人下次還要再打開一次——除非他發現了第二個方塊。
    // 現在開關本身就是偏好，Rust 那邊記住（`mcp::remember`）。
    const w = await open()

    expect(
      w.findAll('.check').some((l) => l.text().includes('自動')),
      '不該再有第二個「而且下次也要」的方塊',
    ).toBe(false)
  })

  it('記住了就要說出來', async () => {
    // 安靜地記住跟安靜地忘記一樣讓人不放心。
    vi.mocked(commands.mcpStatus).mockResolvedValue({
      status: 'ok', data: state({ autostart: true }),
    } as never)
    const w = await open()

    expect(w.text()).toContain('下次開 App')
  })

  it('自動啟用失敗時一定要說', async () => {
    // 不說的話畫面跟「忘了打開」長得一模一樣，而使用者會以為自己上次
    // 忘了勾——那正是這個偏好要消滅的東西。
    vi.mocked(commands.mcpStatus).mockResolvedValue({
      status: 'ok',
      data: state({
        running: false, url: null, autostart: true,
        autostartFailed: '埠 53809 開不起來（可能被別的程式佔用了）',
      }),
    } as never)
    const w = await open()

    expect(w.text()).toContain('沒有自動開起來')
    expect(w.text()).toContain('53809')
  })

  it('狀態同步給標頭那顆燈', async () => {
    // 兩個地方各存一份的話遲早會各說各話。
    const w = await open()
    expect(store.agentRunning).toBe(true)

    vi.mocked(commands.stopMcp).mockResolvedValue({
      status: 'ok', data: state({ running: false, url: null }),
    } as never)
    await w.find('.switch input').trigger('change')
    await flushPromises()
    expect(store.agentRunning).toBe(false)
  })

  it('永遠說得出哪兩道關不掉', async () => {
    // 使用者要分得出「我關掉的是哪一道」。
    const w = await open()
    expect(w.find('.notes').text()).toContain('關不掉')
  })

  it('埠被佔用時把 Rust 的話原樣顯示，並且回頭問一次真正的狀態', async () => {
    vi.mocked(commands.setMcpConfig).mockResolvedValue({
      status: 'error', error: { message: '埠 80 開不起來（可能被別的程式佔用了）' },
    } as never)
    const w = await open()
    vi.mocked(commands.mcpStatus).mockClear()

    await w.find('.field input').setValue('80')
    await w.find('.field input').trigger('change')
    await flushPromises()

    expect(store.error).toContain('被別的程式佔用')
    expect(commands.mcpStatus).toHaveBeenCalled()
  })
})
