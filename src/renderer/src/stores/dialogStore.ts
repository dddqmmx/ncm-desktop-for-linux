import { defineStore } from 'pinia'
import { ref } from 'vue'

export type DialogButtonMode = 'confirm' | 'confirm-cancel'

export interface DialogOptions {
  title: string
  message: string
  mode?: DialogButtonMode
  confirmText?: string
  cancelText?: string
}

export const useDialogStore = defineStore('dialog', () => {
  const isOpen = ref(false)
  const title = ref('')
  const message = ref('')
  const mode = ref<DialogButtonMode>('confirm')
  const confirmText = ref('确定')
  const cancelText = ref('取消')
  let resolver: ((confirmed: boolean) => void) | null = null

  const setDialogContent = (options: DialogOptions): void => {
    title.value = options.title
    message.value = options.message
    mode.value = options.mode ?? 'confirm'
    confirmText.value = options.confirmText ?? '确定'
    cancelText.value = options.cancelText ?? '取消'
  }

  const openFallbackDialog = (options: DialogOptions): Promise<boolean> => {
    setDialogContent(options)
    isOpen.value = true

    return new Promise((resolve) => {
      resolver = resolve
    })
  }

  const open = async (options: DialogOptions): Promise<boolean> => {
    if (typeof window !== 'undefined' && window.api?.open_dialog_window) {
      try {
        return await window.api.open_dialog_window(options)
      } catch (error) {
        console.error('打开独立弹窗失败，回退到内嵌弹窗', error)
      }
    }

    return openFallbackDialog(options)
  }

  const close = (confirmed: boolean): void => {
    isOpen.value = false
    resolver?.(confirmed)
    resolver = null
  }

  const confirm = (): void => close(true)
  const cancel = (): void => close(false)

  return {
    isOpen,
    title,
    message,
    mode,
    confirmText,
    cancelText,
    open,
    confirm,
    cancel
  }
})
