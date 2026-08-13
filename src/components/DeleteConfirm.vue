<script setup lang="ts">
/**
 * 刪除確認框。
 *
 * # 它問的不是「你確定嗎」
 *
 * 「你確定嗎」是沒有資訊的問題，使用者三次之後就會閉著眼睛按確定。
 * 這個框問的是**「這樣會弄壞這些東西，還要刪嗎」**——附上具體清單。
 *
 * 那份清單不是這裡算的，也不是另外寫一套影響分析：Rust 把真的刪除
 * 跑在專案的複本上，再比對 lint 前後的差別。所以框裡寫的，
 * 就是按下去之後真正會發生的事。
 *
 * # 為什麼刪除要這樣慎重
 *
 * 本工具存在的理由就是「怕漏」。少一條連線正是它要抓的東西，
 * 而刪除是唯一會親手製造那種缺漏的操作。
 */
import { ref, watch } from 'vue'
import { useProject } from '../lib/store'
import type { Impact } from '../lib/model'

const store = useProject()
const impact = ref<Impact | null>(null)
const 算著 = ref(false)

watch(
  () => store.刪除中,
  async (目標) => {
    impact.value = null
    if (!目標) return
    算著.value = true
    impact.value = await store.預覽編輯({
      deleteConnection: { environment: 目標.environment, connection: 目標.connection },
    })
    算著.value = false
  },
  { immediate: true },
)
</script>

<template>
  <div v-if="store.刪除中" class="scrim" @click.self="store.刪除中 = null">
    <section class="box" role="dialog" aria-modal="true">
      <h2>刪除這條連線？</h2>
      <p class="mono target">{{ store.刪除中.label }}</p>

      <p v-if="算著" class="muted">正在算會影響什麼…</p>

      <template v-else-if="impact">
        <div v-if="impact.introduced.length" class="danger">
          <p class="lead">刪掉之後會多出這些問題：</p>
          <ul>
            <li v-for="(f, i) in impact.introduced" :key="i">
              <span class="code">{{ f.rule }}</span>
              <span>{{ f.detail }}</span>
            </li>
          </ul>
        </div>
        <p v-else class="muted safe">lint 沒有因此多出任何問題。</p>

        <!-- 少見但可能：刪掉一條指向不存在機器的連線，反而修好一項。 -->
        <p v-if="impact.resolved.length" class="muted">
          同時會解掉 {{ impact.resolved.length }} 項既有的問題。
        </p>
      </template>

      <footer>
        <span class="muted hint">刪錯了可以按 ⌘Z 復原</span>
        <span class="grow" />
        <button @click="store.刪除中 = null">取消</button>
        <button class="danger-btn" :disabled="算著" @click="store.確認刪除()">刪除</button>
      </footer>
    </section>
  </div>
</template>

<style scoped>
.scrim {
  position: fixed;
  inset: 0;
  display: grid;
  place-items: center;
  background: color-mix(in srgb, #000 42%, transparent);
  z-index: 20;
}
.box {
  width: min(560px, 92vw);
  max-height: 80vh;
  overflow: auto;
  padding: 18px 20px 14px;
  border: 1px solid var(--rule);
  border-radius: 8px;
  background: var(--surface);
  display: flex;
  flex-direction: column;
  gap: 10px;
}
h2 { margin: 0; font-size: 15px; font-weight: 600; }
p { margin: 0; }
.target { font-size: 12.5px; color: var(--ink-2); }

.danger {
  padding: 10px 12px;
  border: 1px solid color-mix(in srgb, var(--broken) 40%, transparent);
  border-radius: 5px;
  background: color-mix(in srgb, var(--broken) 8%, transparent);
}
.lead { font-weight: 600; color: var(--broken); margin-bottom: 6px; }
ul { margin: 0; padding-left: 0; list-style: none; display: flex; flex-direction: column; gap: 4px; }
li { display: flex; gap: 8px; font-size: 12.5px; align-items: baseline; }
.code {
  font-family: var(--mono);
  font-size: 11px;
  font-weight: 600;
  color: var(--broken);
  flex: none;
}
.safe { font-size: 12.5px; }

footer { display: flex; align-items: center; gap: 8px; margin-top: 4px; }
.grow { flex: 1; }
.hint { font-size: 11.5px; }
.danger-btn {
  background: var(--broken);
  border-color: var(--broken);
  color: #fff;
  font-weight: 600;
}
</style>
