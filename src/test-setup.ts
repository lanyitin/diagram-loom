/**
 * 測試環境的補丁。
 *
 * happy-dom 沒有提供 `localStorage`（jsdom 有）。真正的 webview 有，
 * 所以這是**測試環境的缺口**，不是程式該防的情況——補在這裡，
 * 不要在 `ui.ts` 裡加一個只有測試會走到的 `typeof` 判斷。
 */
const store = new Map<string, string>()

globalThis.localStorage = {
  getItem: (k: string) => store.get(k) ?? null,
  setItem: (k: string, v: string) => void store.set(k, String(v)),
  removeItem: (k: string) => void store.delete(k),
  clear: () => store.clear(),
  key: (i: number) => [...store.keys()][i] ?? null,
  get length() {
    return store.size
  },
} as Storage
