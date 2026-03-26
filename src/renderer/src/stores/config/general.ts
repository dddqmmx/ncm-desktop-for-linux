import { defineStore } from 'pinia'
import { ref } from 'vue'
import { loadSettings } from './utils'

export const useGeneralConfigStore = defineStore('generalConfig', () => {
  const initialSettings = loadSettings()

  const autoLaunch = ref(initialSettings.autoLaunch)
  const trayMinimize = ref(initialSettings.trayMinimize)

  return {
    autoLaunch,
    trayMinimize
  }
})
