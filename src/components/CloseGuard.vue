<script setup lang="ts">
/**
 * 關視窗前攔一下未儲存的變更。
 *
 * # 為什麼不用系統的 confirm
 *
 * 系統對話框只能問「要不要關」，而使用者真正要的第三個選項是**存了再關**。
 * 少了那個選項，他得先取消、再自己按儲存、再關一次——三步做一件事，
 * 久了就會養成「隨手按儲存」的習慣，而那正是 dirty 標記想省掉的事。
 *
 * # 為什麼在前端做
 *
 * Tauri 的 `onCloseRequested` 攔得到關閉，`preventDefault()` 之後
 * 主導權就在這裡。放 Rust 要多一次事件來回，還得自己開執行緒跑對話框，
 * 換來的只是同一個結果。
 *
 * **判斷本身仍在 Rust**：dirty 是 `History` 算的，這裡只是問話。
 */
import { onMounted, onUnmounted, ref } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useProject } from '../lib/store'

const store = useProject()
const 問著 = ref(false)
let 收工: (() => void) | null = null

onMounted(async () => {
  收工 = await getCurrentWindow().onCloseRequested((e) => {
    if (!store.未儲存) return
    e.preventDefault()
    問著.value = true
  })
})

onUnmounted(() => 收工?.())

/** 真的關掉。`destroy` 不會再觸發 `onCloseRequested`，所以不會繞回來。 */
async function 關掉() {
  問著.value = false
  await getCurrentWindow().destroy()
}

async function 存了再關() {
  await store.儲存()
  // 存檔失敗時**不關**——關掉就真的沒了，而錯誤訊息也會跟著視窗一起消失。
  if (store.錯誤) {
    問著.value = false
    return
  }
  await 關掉()
}
</script>

<template>
  <div v-if="問著" class="scrim">
    <section class="box" role="dialog" aria-modal="true">
      <h2>有還沒存檔的變更</h2>
      <p class="muted">關掉視窗就會不見。</p>
      <footer>
        <button @click="問著 = false">取消</button>
        <span class="grow" />
        <button class="danger-btn" @click="關掉()">不存直接關</button>
        <button class="primary" @click="存了再關()">存檔並關閉</button>
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
  z-index: 30;
}
.box {
  width: min(420px, 92vw);
  padding: 18px 20px 14px;
  border: 1px solid var(--rule);
  border-radius: 8px;
  background: var(--surface);
  display: flex;
  flex-direction: column;
  gap: 10px;
}
h2 { margin: 0; font-size: 15px; font-weight: 600; }
p { margin: 0; font-size: 13px; }
footer { display: flex; align-items: center; gap: 8px; margin-top: 6px; }
.grow { flex: 1; }
.danger-btn { color: var(--broken); border-color: color-mix(in srgb, var(--broken) 45%, transparent); }
</style>
