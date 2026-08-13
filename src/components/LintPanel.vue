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
 */
import { useProject } from '../lib/store'
import type { Finding } from '../lib/model'

const store = useProject()

function 跳過去(f: Finding) {
  store.檢視 = '連線表'
  store.只看有問題 = true
  store.搜尋 = ''
  if (f.environment) store.比對中的環境 = [f.environment]
}
</script>

<template>
  <section class="panel" :class="{ 展開: store.面板展開 }">
    <button class="bar" @click="store.面板展開 = !store.面板展開">
      <span v-if="store.錯誤數" class="tally error">{{ store.錯誤數 }} 錯誤</span>
      <span v-if="store.警告數" class="tally warn">{{ store.警告數 }} 警告</span>
      <span v-if="!store.錯誤數 && !store.警告數" class="tally ok">沒有發現問題</span>
      <span class="grow" />
      <span class="muted">{{ store.面板展開 ? '收合 ▼' : '展開 ▲' }}</span>
    </button>

    <div v-if="store.面板展開" class="list">
      <table>
        <tbody>
          <tr v-for="(f, i) in store.發現" :key="i" @click="跳過去(f)">
            <td class="rule">
              <span :class="['code', f.severity]">{{ f.rule }}</span>
            </td>
            <td class="mono env muted">{{ f.environment ? store.環境名(f.environment) : '邏輯層' }}</td>
            <td class="mono subject">{{ f.subject }}</td>
            <td class="detail">{{ f.detail }}</td>
          </tr>
          <tr v-if="store.發現.length === 0">
            <td class="empty muted">這個專案目前沒有任何缺漏。</td>
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
tbody tr { cursor: default; }

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
.detail { color: var(--ink-2); }
.empty { text-align: center; padding: 28px; }
</style>
