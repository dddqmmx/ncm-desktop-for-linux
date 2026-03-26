import { defineStore } from 'pinia'
import { ref } from 'vue'

export const usePlayerUiStore = defineStore('playerUi', () => {
  const isFullScreen = ref(false)

  const toggleFullScreen = (): void => {
    isFullScreen.value = !isFullScreen.value
  }

  return {
    isFullScreen,
    toggleFullScreen
  }
})
