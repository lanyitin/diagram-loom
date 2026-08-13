import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  plugins: [vue()],

  // Tauri 會用 devUrl 連過來，所以埠號不能被 Vite 自己換掉。
  // strictPort 讓它撞到就直接失敗，而不是靜靜換一個埠讓 App 連不上。
  server: {
    port: 1420,
    strictPort: true,
  },

  // Tauri 的 CLI 會在同一個終端機印訊息，Vite 清畫面會把它蓋掉。
  clearScreen: false,

  build: {
    // 桌面應用不必顧慮舊瀏覽器——WKWebView 與 WebView2 都很新。
    target: 'es2022',
    sourcemap: true,
  },
})
