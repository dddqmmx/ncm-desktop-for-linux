import { defineStore } from 'pinia'
import { ref } from 'vue'
import {
  FullscreenAuxButtonId,
  FullscreenAuxButtonSide,
  FullscreenAuxButtonState,
  ThemeMode
} from './types'
import { loadSettings } from './utils'

const cloneList = (list: FullscreenAuxButtonState[]): FullscreenAuxButtonState[] =>
  list.map((item) => ({ ...item }))

export const useAppearanceConfigStore = defineStore('appearanceConfig', () => {
  const initialSettings = loadSettings()

  const theme = ref<ThemeMode>(initialSettings.theme)
  const acrylic = ref(initialSettings.acrylic)
  const accentColor = ref(initialSettings.accentColor)
  // 左右两列各一份有序列表：拖拽只改顺序，开关只改 visible
  const fullscreenAuxLeft = ref<FullscreenAuxButtonState[]>(
    cloneList(initialSettings.fullscreenAuxLeft)
  )
  const fullscreenAuxRight = ref<FullscreenAuxButtonState[]>(
    cloneList(initialSettings.fullscreenAuxRight)
  )

  const listFor = (side: FullscreenAuxButtonSide): typeof fullscreenAuxLeft =>
    side === 'left' ? fullscreenAuxLeft : fullscreenAuxRight

  /** 顺序 = 全屏播放页该侧的排列顺序 */
  function setFullscreenAuxSide(
    side: FullscreenAuxButtonSide,
    next: FullscreenAuxButtonState[]
  ): void {
    listFor(side).value = cloneList(next)
  }

  function setFullscreenAuxVisible(id: FullscreenAuxButtonId, visible: boolean): void {
    for (const list of [fullscreenAuxLeft.value, fullscreenAuxRight.value]) {
      const target = list.find((item) => item.id === id)
      if (target) target.visible = visible
    }
  }

  return {
    theme,
    acrylic,
    accentColor,
    fullscreenAuxLeft,
    fullscreenAuxRight,
    setFullscreenAuxSide,
    setFullscreenAuxVisible
  }
})
