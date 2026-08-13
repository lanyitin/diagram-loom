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
  Cell, Edit, Environment, Finding, FixValue, Id, Impact, Relationship, Resource, Row,
  Snapshot,
} from './model'

type view = '覆蓋矩陣' | '連線表' | '資源'

interface State {
  snapshot: Snapshot | null
  view: view
  panelOpen: boolean
  importing: boolean
  /** 使用者勾選要比對哪幾個環境。空陣列代表「全部」。 */
  selectedEnvironments: Id[]
  search: string
  onlyProblems: boolean
  /**
   * 從 lint 面板點過來時，只留跟這個元素有關的列。
   *
   * 是元素 id 而不是連線 id：發現的 subject 可能是 Endpoint（L006）
   * 或 Instance（L008）。「這個 id 對應到哪幾列」由 Rust 的
   * `Row.subjects` 回答，前端只做比對。
   */
  focus: FocusTarget | null
  busy: boolean
  error: string | null
  /** 使用者按了刪除、還沒確認的那一條。`null` 表示沒有對話框。 */
  deleting: PendingDelete | null
  /** 使用者要補一條連線給哪個環境的哪條契約。 */
  addingConnection: PendingConnection | null
  /** 使用者要在哪個環境批次建立哪個服務的機器。 */
  addingInstances: PendingInstances | null
  /** 使用者正在新增或編輯的資源。 */
  editingResource: PendingEdit | null
  /** 「讓 AI 助手接進來」那個面板開著。 */
  agentPanelOpen: boolean
}

/** 資源表單需要知道的：改哪一個、是不是新的、怎麼稱呼它。 */
export interface PendingEdit {
  resource: Resource
  isNew: boolean
  /** 標題用：「服務」「契約」。就是分頁上的那個字。 */
  kind: string
}

/** 批次建立的表單需要知道的：建哪個服務的機器，以及怎麼稱呼它。 */
export interface PendingInstances {
  environment: Id
  container: Id
  label: string
}

/** 新增連線的表單需要知道的：補給誰，以及怎麼稱呼它。 */
export interface PendingConnection {
  environment: Id
  relationship: Id
  label: string
}

/** 從哪一項發現跳過來的。標籤是給畫面上那顆「取消聚焦」的膠囊用的。 */
export interface FocusTarget {
  subject: Id
  label: string
}

/**
 * 刪除確認框需要知道的：刪什麼，以及怎麼稱呼它。
 *
 * 帶的是一個完整的 `Edit`，不是「連線 id」——連線、服務、契約、機器
 * 都可以刪，而確認框問的問題（「會弄壞什麼」）對每一種都一樣。
 */
export interface PendingDelete {
  edit: Edit
  /** 標題用：「連線」「服務」「契約」。呼叫端本來就知道自己在刪什麼。 */
  kind: string
  /** 那個東西叫什麼，顯示在標題下面。 */
  label: string
}

export const useProject = defineStore('project', {
  state: (): State => ({
    snapshot: null,
    view: '覆蓋矩陣',
    panelOpen: false,
    importing: false,
    selectedEnvironments: [],
    search: '',
    onlyProblems: false,
    focus: null,
    busy: false,
    error: null,
    deleting: null,
    addingConnection: null,
    addingInstances: null,
    editingResource: null,
    agentPanelOpen: false,
  }),

  getters: {
    isOpen: (s) => s.snapshot !== null,

    environments: (s): Environment[] => s.snapshot?.project.environments ?? [],

    relationships: (s): Relationship[] => s.snapshot?.project.logical.relationships ?? [],

    /** 目前要顯示的環境欄位。沒勾就是全部。 */
    visibleEnvironments(): Environment[] {
      if (this.selectedEnvironments.length === 0) return this.environments
      const picked = new Set(this.selectedEnvironments)
      return this.environments.filter((e) => picked.has(e.id))
    },

    /** 依搜尋與「只看有問題」篩過的列。 */
    visibleRelationships(): Relationship[] {
      const keyword = this.search.trim().toLowerCase()
      const envIds = new Set(this.visibleEnvironments.map((e) => e.id))

      return this.relationships.filter((rel) => {
        if (keyword && !`${rel.slug} ${rel.purpose}`.toLowerCase().includes(keyword)) {
          return false
        }
        if (!this.onlyProblems) return true
        return this.cells
          .filter((c) => c.relationship === rel.id && envIds.has(c.environment))
          .some((c) => c.status !== 'realized')
      })
    },

    cells: (s): Cell[] => s.snapshot?.matrix.cells ?? [],

    rows: (s): Row[] => s.snapshot?.rows ?? [],

    /** 連線表要顯示的列。跟矩陣共用同一組篩選條件，切換檢視時不會突然變一套。 */
    visibleRows(): Row[] {
      const keyword = this.search.trim().toLowerCase()
      const envIds = new Set(this.visibleEnvironments.map((e) => e.id))

      return this.rows.filter((r) => {
        if (!envIds.has(r.environment)) return false
        // 聚焦跟其他條件是 AND，不是取代。跳過去時會先把其他條件清乾淨，
        // 所以當下只有它在作用；之後再搜尋就是在這幾列裡面再縮小，不會跳來跳去。
        if (this.focus && !r.subjects.includes(this.focus.subject)) return false
        if (this.onlyProblems && r.severity === null) return false
        if (!keyword) return true
        const haystack = [
          r.servesSlug ?? r.serves, r.purpose,
          r.from.label, r.to.label,
          ...r.to.addresses,
        ].join(' ').toLowerCase()
        return haystack.includes(keyword)
      })
    },

    findings: (s): Finding[] => s.snapshot?.findings ?? [],

    errorCount(): number {
      return this.findings.filter((f) => f.severity === 'error').length
    },

    warningCount(): number {
      return this.findings.filter((f) => f.severity === 'warning').length
    },

    /** 內容跟磁碟上不一樣。是 Rust 算的——前端沒有第二套判斷。 */
    dirty: (s): boolean => s.snapshot?.dirty ?? false,

    undoLabel: (s): string | null => s.snapshot?.undoLabel ?? null,
    redoLabel: (s): string | null => s.snapshot?.redoLabel ?? null,
  },

  actions: {
    cell(relationship: Id, environment: Id): Cell | undefined {
      return this.cells.find(
        (c) => c.relationship === relationship && c.environment === environment,
      )
    },

    envName(id: Id): string {
      return this.environments.find((e) => e.id === id)?.slug ?? id
    },

    async open(path: string) {
      await this.run(() => commands.openProject(path))
    },

    /** 在一個空資料夾裡開新專案。資料夾非空時 Rust 會擋下來。 */
    async createProject(path: string, name: string) {
      await this.run(() => commands.createProject(path, name))
      // 空專案沒有連線也沒有契約，矩陣是一片空白。直接帶到資源檢視，
      // 那裡每張表都會說「這是什麼、為什麼需要它」。
      if (this.isOpen) this.view = '資源'
    },

    async recheck() {
      if (!this.isOpen) return
      await this.run(() => commands.recheck())
    },

    async save() {
      if (!this.isOpen) return
      await this.run(() => commands.saveProject())
    },

    async applyEdit(edit: Edit) {
      await this.run(() => commands.applyEdit(edit))
    },

    /**
     * 照著發現的 `fix` 填了一格，送回去。
     *
     * 送的是**發現本身 + 填的值**，不是一個 Edit——哪條規則要寫進哪個欄位
     * 由 Rust 的 `edit_for` 決定。這裡多一行對照表，就是第二套規則的開始。
     */
    async applyFix(finding: Finding, value: FixValue) {
      await this.run(() => commands.applyFix(finding, value))
    },

    /**
     * 「這樣改會弄壞什麼」。刪除之前先問這個。
     *
     * 跟其他 action 不同，它**不換掉 snapshot**——它什麼都沒改。
     * 失敗時回 null，呼叫端就不要往下走。
     */
    async previewEdit(edit: Edit): Promise<Impact | null> {
      const res = await commands.previewEdit(edit)
      if (res.status === 'ok') return res.data
      this.error = (res.error as { message?: string })?.message ?? String(res.error)
      return null
    },

    /** 確認刪除。對話框在此之前已經把影響給使用者看過了。 */
    async confirmDelete() {
      const target = this.deleting
      if (!target) return
      this.deleting = null
      await this.applyEdit(target.edit)
    },

    async undo() {
      if (!this.undoLabel) return
      await this.run(() => commands.undo())
    },

    async redo() {
      if (!this.redoLabel) return
      await this.run(() => commands.redo())
    },

    /** 三個 command 的錯誤處理與忙碌狀態都一樣，集中在這裡。 */
    async run(call: () => Promise<{ status: 'ok' | 'error'; data?: unknown; error?: unknown }>) {
      this.busy = true
      this.error = null
      try {
        const res = await call()
        if (res.status === 'ok') {
          this.snapshot = res.data as Snapshot
        } else {
          this.error = (res.error as { message?: string })?.message ?? String(res.error)
        }
      } catch (e) {
        this.error = e instanceof Error ? e.message : String(e)
      } finally {
        this.busy = false
      }
    },
  },
})
