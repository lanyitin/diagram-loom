import { defineConfig, mergeConfig } from 'vitest/config'
import base from './vite.config.ts'

export default mergeConfig(
  base,
  defineConfig({
    test: {
      // 元件測試需要 DOM。happy-dom 比 jsdom 快得多，而我們只用到最基本的部分。
      environment: 'happy-dom',
      include: ['src/**/*.test.ts'],
    },
  }),
)
