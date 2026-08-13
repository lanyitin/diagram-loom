<script setup lang="ts">
/**
 * Lint 面板：從底部拉起。
 *
 * 常駐一條摘要列，點開才佔空間——這個工具會被開一整天，
 * 大部分時間使用者在看表格，不是在看錯誤清單。
 *
 * # 為什麼可以點
 *
 * 「L004 萬用字元期望 4 個」告訴你有問題，但沒告訴你在哪。
 * 點一下就跳到那個環境的連線表並且只留有問題的列——
 * 從「知道有錯」到「看到那一列」不該需要自己找。
 *
 * # 為什麼可以就地修
 *
 * 大部分的發現只差一格：位址、用途、期望數量、一個開關。
 * 若要先跳到表格、找到那一列、再找到那一格，使用者會累到乾脆不修，
 * 然後開始忽略 lint——那本工具就沒價值了。
 *
 * **這裡沒有任何「哪條規則要填什麼」的判斷。** 該長出什麼控制項寫在
 * `finding.fix` 裡（Rust 算的），填完把值原封不動送回去，
 * 由 Rust 的 `edit_for` 決定要寫進哪個欄位。
 */
import { ref } from 'vue'
import { useProject } from '../lib/store'
import FixEditor from './FixEditor.vue'
import type { Finding } from '../lib/model'

const store = useProject()

/** 目前展開修法的那一項。用索引就好——清單每次 lint 都會重算。 */
const editingIndex = ref<number | null>(null)

/**
 * 跳到這一項發現指的那幾列。
 *
 * 之前只切到「那個環境的有問題的列」，於是同一個環境裡連點兩項，
 * 畫面一模一樣——看起來就像第二次點沒反應。現在會聚焦到那一項本身。
 *
 * 「這個 subject 對應到哪幾列」不在這裡判斷，由 Rust 的 `Row.subjects` 回答。
 */
/**
 * 開「補一條連線」的表單。
 *
 * 要補哪條契約寫在 `fix.addConnection` 裡（Rust 決定的），
 * 這裡只負責把它遞出去——同樣不認得規則代號。
 */
function openAddConnection(f: Finding) {
  if (!f.fix?.addConnection || !f.environment) return
  store.addingConnection = {
    environment: f.environment,
    relationship: f.fix.addConnection.relationship,
    label: f.detail,
  }
}

/** 開「批次建立機器」的表單。同樣不認得規則代號。 */
function openAddInstances(f: Finding) {
  if (!f.fix?.addInstances || !f.environment) return
  store.addingInstances = {
    environment: f.environment,
    container: f.fix.addInstances.container,
    label: f.detail,
  }
}

function jumpTo(f: Finding) {
  store.view = '連線表'
  store.search = ''
  store.onlyProblems = false
  store.focus = { subject: f.subject, label: `${f.rule} ${f.detail}` }
  // 邏輯層的發現沒有環境，這時不要動環境勾選——它跟環境無關。
  if (f.environment) store.selectedEnvironments = [f.environment]
}
</script>

<template>
  <section class="panel" :class="{ expanded: store.panelOpen }">
    <button class="bar" @click="store.panelOpen = !store.panelOpen">
      <span v-if="store.errorCount" class="tally error">{{ store.errorCount }} 錯誤</span>
      <span v-if="store.warningCount" class="tally warn">{{ store.warningCount }} 警告</span>
      <span v-if="!store.errorCount && !store.warningCount" class="tally ok">沒有發現問題</span>
      <span class="grow" />
      <span class="muted">{{ store.panelOpen ? '收合 ▼' : '展開 ▲' }}</span>
    </button>

    <div v-if="store.panelOpen" class="list">
      <table>
        <tbody>
          <!-- key 要帶上 end：兩端都是萬用字元的連線會產生兩項 rule 與 subject
               完全相同的 L004，少了 end 兩列就會撞 key。 -->
          <template
            v-for="(f, i) in store.findings"
            :key="`${f.rule}-${f.environment}-${f.subject}-${f.end}`"
          >
            <tr>
              <td class="rule">
                <span :class="['code', f.severity]">{{ f.rule }}</span>
              </td>
              <td class="mono env muted">{{ f.environment ? store.envName(f.environment) : '邏輯層' }}</td>
              <td class="mono subject">{{ f.subject }}</td>
              <td class="detail" @click="jumpTo(f)">{{ f.detail }}</td>
              <td class="act">
                <!-- 這兩種要開一張表單，不是就地填一格，所以走另一顆鈕。 -->
                <button
                  v-if="f.fix?.addConnection" class="fix" :disabled="store.busy"
                  @click="openAddConnection(f)"
                >補連線…</button>
                <button
                  v-else-if="f.fix?.addInstances" class="fix" :disabled="store.busy"
                  @click="openAddInstances(f)"
                >建機器…</button>
                <button
                  v-else-if="f.fix" class="fix" :disabled="store.busy"
                  @click="editingIndex = editingIndex === i ? null : i"
                >
                  {{ editingIndex === i ? '取消' : '修…' }}
                </button>
                <!-- 剩下沒有修法的（L003：指到了不存在的東西）不放假按鈕。
                     一個按下去只會跳「這個還沒做」的按鈕，比沒有按鈕更糟。 -->
                <span v-else class="muted hint">要改接</span>
              </td>
            </tr>

            <tr
              v-if="editingIndex === i && f.fix && !f.fix.addConnection && !f.fix.addInstances"
              class="editor"
            >
              <td colspan="5">
                <FixEditor :finding="f" :fix="f.fix" @done="editingIndex = null" />
              </td>
            </tr>
          </template>

          <tr v-if="store.findings.length === 0">
            <td class="empty muted" colspan="5">這個專案目前沒有任何缺漏。</td>
          </tr>
        </tbody>
      </table>
    </div>
  </section>
</template>

<style scoped>
.panel { border-top: 1px solid var(--rule); background: var(--surface-2); }
.panel.展開 { display: flex; flex-direction: column; max-height: 44vh; }

.bar {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 8px 14px;
  border: 0;
  border-radius: 0;
  background: transparent;
  font-size: 12.5px;
  text-align: left;
}
.bar:hover { background: var(--surface); }
.grow { flex: 1; }

.tally { font-weight: 600; }
.tally.error { color: var(--broken); }
.tally.warn { color: var(--warn); }
.tally.ok { color: var(--ok); }

.list { overflow: auto; border-top: 1px solid var(--rule); background: var(--surface); }

table { border-collapse: separate; border-spacing: 0; width: 100%; }
td {
  padding: 5px 12px;
  border-bottom: 1px solid var(--rule-2);
  vertical-align: top;
  font-size: 13px;
}
tbody tr:hover td { background: var(--surface-2); }

.rule { width: 58px; }
.code {
  font-family: var(--mono);
  font-size: 11.5px;
  font-weight: 600;
  padding: 1px 6px;
  border-radius: 3px;
  border: 1px solid currentColor;
}
.code.error { color: var(--broken); }
.code.warning { color: var(--warn); }
.code.info { color: var(--ink-3); }

.env { width: 72px; }
.subject { width: 210px; overflow: hidden; text-overflow: ellipsis; }
.detail { color: var(--ink-2); cursor: pointer; }
.act { width: 92px; text-align: right; }
.hint { font-size: 11.5px; }
.empty { text-align: center; padding: 28px; }

.fix { padding: 1px 8px; font-size: 12px; }

/* 修法就地展開。縮排對齊 detail 欄，看得出它屬於上面那一列。 */
.editor td { background: var(--surface-2); padding: 8px 12px 10px 344px; }
</style>
