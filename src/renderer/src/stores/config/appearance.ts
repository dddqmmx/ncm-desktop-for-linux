import { defineStore } from 'pinia'
import { ref } from 'vue'
import { ThemeMode } from './types'
import { loadSettings } from './utils'

export const useAppearanceConfigStore = defineStore('appearanceConfig', () => {
  const initialSettings = loadSettings()

  const theme = ref<ThemeMode>(initialSettings.theme)
  const acrylic = ref(initialSettings.acrylic)
  const accentColor = ref(initialSettings.accentColor)

  return {
    theme,
    acrylic,
    accentColor
  }
})
