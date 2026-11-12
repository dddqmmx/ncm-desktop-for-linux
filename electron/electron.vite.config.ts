import { resolve } from 'path'
import { defineConfig } from 'electron-vite'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  main: {
    // plugins: [externalizeDepsPlugin()] <-- 注释掉
  },
  preload: {
    // plugins: [externalizeDepsPlugin()] <-- 注释掉
  },
  renderer: {
    resolve: {
      alias: {
        '@renderer': resolve('src/renderer/src')
      }
    },
    plugins: [vue()]
  }
})
