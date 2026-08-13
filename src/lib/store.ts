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
import type {
  Cell, Edit, Environment, Finding, FixValue, Id, Impact, Relationship, Row, Snapshot,
} from './model'

type 檢視 = '覆蓋矩陣' | '連線表'

interface State {
  snapshot: Snapshot | null
  檢視: 檢視
  面板展開: boolean
  匯入中: boolean
  /** 使用者勾選要比對哪幾個環境。空陣列代表「全部」。 */
  比對中的環境: Id[]
  搜尋: string
  只看有問題: boolean
  /**
   * 從 lint 面板點過來時，只留跟這個元素有關的列。
   *
   * 是元素 id 而不是連線 id：發現的 subject 可能是 Endpoint（L006）
   * 或 Instance（L008）。「這個 id 對應到哪幾列」由 Rust 的
   * `Row.subjects` 回答，前端只做比對。
   */
  聚焦: 聚焦目標 | null
  忙碌中: boolean
  錯誤: string | null
  /** 使用者按了刪除、還沒確認的那一條。`null` 表示沒有對話框。 */
  刪除中: 待刪 | null
  /** 使用者要補一條連線給哪個環境的哪條契約。 */
  新增連線中: 待建 | null
  /** 使用者要在哪個環境批次建立哪個服務的機器。 */
  新增機器中: 待建機器 | null
}

/** 批次建立的表單需要知道的：建哪個服務的機器，以及怎麼稱呼它。 */
export interface 待建機器 {
  environment: Id
  container: Id
  label: string
}

/** 新增連線的表單需要知道的：補給誰，以及怎麼稱呼它。 */
export interface 待建 {
  environment: Id
  relationship: Id
  label: string
}

/** 從哪一項發現跳過來的。標籤是給畫面上那顆「取消聚焦」的膠囊用的。 */
export interface 聚焦目標 {
  subject: Id
  label: string
}

/** 刪除確認框需要知道的：刪什麼，以及怎麼稱呼它。 */
export interface 待刪 {
  environment: Id
  connection: Id
  label: string
}

export const useProject = defineStore('project', {
  state: (): State => ({
    snapshot: null,
    檢視: '覆蓋矩陣',
    面板展開: false,
    匯入中: false,
    比對中的環境: [],
    搜尋: '',
    只看有問題: false,
    聚焦: null,
    忙碌中: false,
    錯誤: null,
    刪除中: null,
    新增連線中: null,
    新增機器中: null,
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
        // 聚焦跟其他條件是 AND，不是取代。跳過去時會先把其他條件清乾淨，
        // 所以當下只有它在作用；之後再搜尋就是在這幾列裡面再縮小，不會跳來跳去。
        if (this.聚焦 && !r.subjects.includes(this.聚焦.subject)) return false
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

    /** 內容跟磁碟上不一樣。是 Rust 算的——前端沒有第二套判斷。 */
    未儲存: (s): boolean => s.snapshot?.dirty ?? false,

    可復原: (s): string | null => s.snapshot?.undoLabel ?? null,
    可重做: (s): string | null => s.snapshot?.redoLabel ?? null,
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

    async 套用編輯(edit: Edit) {
      await this.執行(() => commands.applyEdit(edit))
    },

    /**
     * 照著發現的 `fix` 填了一格，送回去。
     *
     * 送的是**發現本身 + 填的值**，不是一個 Edit——哪條規則要寫進哪個欄位
     * 由 Rust 的 `edit_for` 決定。這裡多一行對照表，就是第二套規則的開始。
     */
    async 修好(finding: Finding, value: FixValue) {
      await this.執行(() => commands.applyFix(finding, value))
    },

    /**
     * 「這樣改會弄壞什麼」。刪除之前先問這個。
     *
     * 跟其他 action 不同，它**不換掉 snapshot**——它什麼都沒改。
     * 失敗時回 null，呼叫端就不要往下走。
     */
    async 預覽編輯(edit: Edit): Promise<Impact | null> {
      const 回應 = await commands.previewEdit(edit)
      if (回應.status === 'ok') return 回應.data
      this.錯誤 = (回應.error as { message?: string })?.message ?? String(回應.error)
      return null
    },

    /** 確認刪除。對話框在此之前已經把影響給使用者看過了。 */
    async 確認刪除() {
      const 目標 = this.刪除中
      if (!目標) return
      this.刪除中 = null
      await this.套用編輯({
        deleteConnection: { environment: 目標.environment, connection: 目標.connection },
      })
    },

    async 復原() {
      if (!this.可復原) return
      await this.執行(() => commands.undo())
    },

    async 重做() {
      if (!this.可重做) return
      await this.執行(() => commands.redo())
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
