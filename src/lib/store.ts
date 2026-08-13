/**
 * 模型的唯讀鏡像。
 *
 * 這裡**不做任何判斷**。什麼叫缺漏、覆蓋矩陣長什麼樣，全部由 Rust 算好送過來。
 * 前端只負責挑出要畫的東西，以及記住「使用者現在在看什麼」。
 *
 * 一旦這個檔案開始出現 `if (segments === 0) return '未實現'` 這種東西，
 * 就是規則漏到前端了——那會演化成第二套「什麼叫缺漏」的標準。
 */

import { defineStore } from 'pinia'
import { commands } from './bindings'
import type { Cell, Environment, Finding, Id, Relationship, Row, Snapshot } from './model'

type 檢視 = '覆蓋矩陣' | '連線表'

interface State {
  snapshot: Snapshot | null
  檢視: 檢視
  面板展開: boolean
  /** 使用者勾選要比對哪幾個環境。空陣列代表「全部」。 */
  比對中的環境: Id[]
  搜尋: string
  只看有問題: boolean
  忙碌中: boolean
  錯誤: string | null
}

export const useProject = defineStore('project', {
  state: (): State => ({
    snapshot: null,
    檢視: '覆蓋矩陣',
    面板展開: false,
    比對中的環境: [],
    搜尋: '',
    只看有問題: false,
    忙碌中: false,
    錯誤: null,
  }),

  getters: {
    已開啟: (s) => s.snapshot !== null,

    環境: (s): Environment[] => s.snapshot?.project.environments ?? [],

    契約: (s): Relationship[] => s.snapshot?.project.logical.relationships ?? [],

    /** 目前要顯示的環境欄位。沒勾就是全部。 */
    顯示的環境(): Environment[] {
      if (this.比對中的環境.length === 0) return this.環境
      const 選了 = new Set(this.比對中的環境)
      return this.環境.filter((e) => 選了.has(e.id))
    },

    /** 依搜尋與「只看有問題」篩過的列。 */
    顯示的契約(): Relationship[] {
      const 關鍵字 = this.搜尋.trim().toLowerCase()
      const 環境們 = new Set(this.顯示的環境.map((e) => e.id))

      return this.契約.filter((rel) => {
        if (關鍵字 && !`${rel.slug} ${rel.purpose}`.toLowerCase().includes(關鍵字)) {
          return false
        }
        if (!this.只看有問題) return true
        return this.格子們
          .filter((c) => c.relationship === rel.id && 環境們.has(c.environment))
          .some((c) => c.status !== 'realized')
      })
    },

    格子們: (s): Cell[] => s.snapshot?.matrix.cells ?? [],

    列: (s): Row[] => s.snapshot?.rows ?? [],

    /** 連線表要顯示的列。跟矩陣共用同一組篩選條件，切換檢視時不會突然變一套。 */
    顯示的列(): Row[] {
      const 關鍵字 = this.搜尋.trim().toLowerCase()
      const 環境們 = new Set(this.顯示的環境.map((e) => e.id))

      return this.列.filter((r) => {
        if (!環境們.has(r.environment)) return false
        if (this.只看有問題 && r.severity === null) return false
        if (!關鍵字) return true
        const 可搜尋 = [
          r.servesSlug ?? r.serves, r.purpose,
          r.from.label, r.to.label,
          ...r.to.addresses,
        ].join(' ').toLowerCase()
        return 可搜尋.includes(關鍵字)
      })
    },

    發現: (s): Finding[] => s.snapshot?.findings ?? [],

    錯誤數(): number {
      return this.發現.filter((f) => f.severity === 'error').length
    },

    警告數(): number {
      return this.發現.filter((f) => f.severity === 'warning').length
    },
  },

  actions: {
    格子(relationship: Id, environment: Id): Cell | undefined {
      return this.格子們.find(
        (c) => c.relationship === relationship && c.environment === environment,
      )
    },

    環境名(id: Id): string {
      return this.環境.find((e) => e.id === id)?.slug ?? id
    },

    async 開啟(path: string) {
      await this.執行(() => commands.openProject(path))
    },

    async 重新檢查() {
      if (!this.已開啟) return
      await this.執行(() => commands.recheck())
    },

    async 儲存() {
      if (!this.已開啟) return
      await this.執行(() => commands.saveProject())
    },

    /** 三個 command 的錯誤處理與忙碌狀態都一樣，集中在這裡。 */
    async 執行(呼叫: () => Promise<{ status: 'ok' | 'error'; data?: unknown; error?: unknown }>) {
      this.忙碌中 = true
      this.錯誤 = null
      try {
        const 回應 = await 呼叫()
        if (回應.status === 'ok') {
          this.snapshot = 回應.data as Snapshot
        } else {
          this.錯誤 = (回應.error as { message?: string })?.message ?? String(回應.error)
        }
      } catch (e) {
        this.錯誤 = e instanceof Error ? e.message : String(e)
      } finally {
        this.忙碌中 = false
      }
    },
  },
})
