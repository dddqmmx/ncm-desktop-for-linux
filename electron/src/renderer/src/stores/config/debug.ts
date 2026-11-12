import { defineStore } from 'pinia'
import { ref } from 'vue'
import { loadSettings } from './utils'

export const useDebugConfigStore = defineStore('debugConfig', () => {
  const initialSettings = loadSettings()

  const lyricDebug = ref(initialSettings.lyricDebug)

  return {
    lyricDebug
  }
})
