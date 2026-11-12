import { ref } from 'vue'
import { usePlayerStore } from '@renderer/stores/playerStore'

/**
 * 进度条拖拽 seek 的共享交互逻辑（PlayerBar / PlayerFullscreen 共用）。
 * input 事件只更新 UI 时间并置起 isSeeking，change 事件提交真正的 seek；
 * 结束后延迟 50ms 释放 isSeeking，让进度同步先吸收 seek 后的新锚点，
 * 避免旧进度数据造成 UI 回弹。
 *
 * isSeeking 是全局唯一的 store 标志，延迟释放的定时器槽也必须全局唯一：
 * 若每个组件实例各持一个定时器，全屏页松手后立刻去拖底栏进度条，
 * 全屏页实例的旧定时器会把进行中的新拖拽提前放行。放在模块作用域，
 * 任何实例的 beginSeek 都能取消上一个实例安排的释放。
 */
let endSeekTimer: number | undefined

export function useSeekControl(logTag: string): {
  handleInput: (e: Event) => void
  handleSeek: (e: Event) => Promise<void>
} {
  const playerStore = usePlayerStore()
  const isDragging = ref(false)

  const beginSeek = (): void => {
    // 新交互开始，上一次拖拽（含其他组件实例）安排的延迟释放作废。
    if (endSeekTimer !== undefined) {
      window.clearTimeout(endSeekTimer)
      endSeekTimer = undefined
    }
    if (!isDragging.value) isDragging.value = true
    if (!playerStore.isSeeking) {
      console.log(`[${logTag}] beginSeek: isSeeking -> true`)
      playerStore.isSeeking = true
    }
  }

  const endSeek = (): void => {
    console.log(`[${logTag}] endSeek: scheduling isSeeking=false in 50ms`)
    endSeekTimer = window.setTimeout(() => {
      endSeekTimer = undefined
      console.log(
        `[${logTag}] endSeek TIMER: isSeeking -> false, isLoading=`,
        playerStore.isLoading
      )
      isDragging.value = false
      playerStore.isSeeking = false
    }, 50)
  }

  const handleInput = (e: Event): void => {
    beginSeek()
    playerStore.currentTime = Number((e.target as HTMLInputElement).value)
  }

  const handleSeek = async (e: Event): Promise<void> => {
    beginSeek()
    const targetTime = Number((e.target as HTMLInputElement).value)
    console.log(`[${logTag}] handleSeek: target=${targetTime}ms`)
    try {
      await playerStore.seek(targetTime)
    } finally {
      endSeek()
    }
  }

  return { handleInput, handleSeek }
}
