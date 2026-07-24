<script setup lang="ts">
import AppIcon from '@renderer/components/common/AppIcon.vue'
import { ref } from 'vue'
import { storeToRefs } from 'pinia'
import SettingGroup from '@renderer/components/settings/SettingGroup.vue'
import { useConfigStore } from '@renderer/stores/configStore'
import { useLocalMusicStore } from '@renderer/stores/localMusicStore'

const configStore = useConfigStore()
const localMusicStore = useLocalMusicStore()
const { libPaths } = storeToRefs(configStore)

const message = ref('')
const messageType = ref<'success' | 'error'>('success')
const isBusy = ref(false)

const setMessage = (type: 'success' | 'error', text: string): void => {
  messageType.value = type
  message.value = text
}

const scanAndImport = async (paths: string[]): Promise<void> => {
  const scanResult = await window.api.scan_library_folders(paths)
  if (scanResult.files.length === 0) {
    setMessage('error', '未在所选文件夹中找到可导入的音频文件。')
    return
  }

  const importResult = await localMusicStore.importPaths(
    scanResult.files.map((file) => ({
      filePath: file.filePath,
      fileName: file.fileName,
      duration: file.duration
    }))
  )

  const parts = [`扫描 ${scanResult.files.length} 首`, `新增 ${importResult.imported} 首`]
  if (importResult.skipped > 0) parts.push(`跳过 ${importResult.skipped} 首`)
  if (importResult.failed.length > 0) parts.push(`失败 ${importResult.failed.length} 首`)
  if (scanResult.truncated) parts.push('结果已截断')

  setMessage(
    importResult.imported > 0 || importResult.skipped > 0 ? 'success' : 'error',
    `${parts.join('，')}。`
  )
}

const formatError = (error: unknown): string => {
  if (error instanceof Error && error.message) return error.message
  return String(error || '未知错误')
}

const addLibraryPath = async (): Promise<void> => {
  if (isBusy.value || localMusicStore.isImporting) return

  if (typeof window.api?.select_library_folder !== 'function') {
    setMessage('error', '曲库接口未就绪，请重启应用后再试。')
    return
  }

  isBusy.value = true
  try {
    const selected = await window.api.select_library_folder()
    if (!selected) return

    const added = configStore.addLibraryPath(selected)
    if (!added) {
      setMessage('error', '路径为空或已经存在。')
      return
    }

    await scanAndImport([selected])
  } catch (error) {
    console.error('添加曲库文件夹失败', error)
    setMessage('error', `添加曲库文件夹失败：${formatError(error)}`)
  } finally {
    isBusy.value = false
  }
}

const rescanLibraryPath = async (path: string): Promise<void> => {
  if (isBusy.value || localMusicStore.isImporting) return

  isBusy.value = true
  try {
    await scanAndImport([path])
  } catch (error) {
    console.error('重新扫描曲库失败', error)
    setMessage('error', '重新扫描失败。')
  } finally {
    isBusy.value = false
  }
}

const removeLibraryPath = async (path: string): Promise<void> => {
  if (isBusy.value) return

  isBusy.value = true
  try {
    configStore.removeLibraryPath(path)
    const removed = localMusicStore.removeSongsUnderPath(path)
    setMessage(
      'success',
      removed > 0 ? `文件夹已移除，并清理 ${removed} 首本地音乐。` : '文件夹已从曲库列表移除。'
    )
  } finally {
    isBusy.value = false
  }
}
</script>

<template>
  <div class="settings-tab">
    <SettingGroup
      title="本地文件夹"
      tip="添加文件夹后会自动递归扫描音频并加入本地音乐列表；移除文件夹会同步清理对应歌曲。"
      no-card
    >
      <div class="settings-path-list">
        <div v-if="libPaths.length === 0" class="settings-empty-state">
          还没有添加本地音乐目录。
        </div>

        <div v-for="path in libPaths" :key="path" class="settings-path-item">
          <AppIcon name="folder" :size="16" />
          <span class="settings-path-label">{{ path }}</span>
          <button
            class="settings-inline-action-btn"
            :disabled="isBusy || localMusicStore.isImporting"
            @click="rescanLibraryPath(path)"
          >
            重新扫描
          </button>
          <button
            class="settings-remove-path"
            :disabled="isBusy"
            @click="removeLibraryPath(path)"
          >
            移除
          </button>
        </div>

        <button
          class="settings-add-path-btn"
          :disabled="isBusy || localMusicStore.isImporting"
          @click="addLibraryPath"
        >
          {{ isBusy || localMusicStore.isImporting ? '正在处理...' : '+ 添加文件夹' }}
        </button>

        <p v-if="message" class="settings-status" :class="messageType">
          {{ message }}
        </p>
      </div>
    </SettingGroup>
  </div>
</template>

<style scoped></style>
