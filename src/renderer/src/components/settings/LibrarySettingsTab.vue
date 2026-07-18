<script setup lang="ts">
import AppIcon from '@renderer/components/common/AppIcon.vue'
import { ref } from 'vue'
import { storeToRefs } from 'pinia'
import SettingGroup from '@renderer/components/settings/SettingGroup.vue'
import { useConfigStore } from '@renderer/stores/configStore'

const configStore = useConfigStore()
const { libPaths } = storeToRefs(configStore)

const message = ref('')
const messageType = ref<'success' | 'error'>('success')

const addLibraryPath = (): void => {
  const input = window.prompt('请输入本地音乐文件夹路径')
  const path = input?.trim()

  if (!path) {
    return
  }

  const added = configStore.addLibraryPath(path)
  messageType.value = added ? 'success' : 'error'
  message.value = added ? '文件夹已加入曲库列表。' : '路径为空或已经存在。'
}

const removeLibraryPath = (path: string): void => {
  configStore.removeLibraryPath(path)
  messageType.value = 'success'
  message.value = '文件夹已从曲库列表移除。'
}
</script>

<template>
  <div class="settings-tab">
    <SettingGroup
      title="本地文件夹"
      tip="当前版本暂未接入系统目录选择器，点击按钮后可手动输入绝对路径。"
      no-card
    >
      <div class="settings-path-list">
        <div v-if="libPaths.length === 0" class="settings-empty-state">
          还没有添加本地音乐目录。
        </div>

        <div v-for="path in libPaths" :key="path" class="settings-path-item">
          <AppIcon name="folder" :size="16" />
          <span class="settings-path-label">{{ path }}</span>
          <button class="settings-remove-path" @click="removeLibraryPath(path)">移除</button>
        </div>

        <button class="settings-add-path-btn" @click="addLibraryPath">+ 添加文件夹</button>
      </div>
    </SettingGroup>
  </div>
</template>

<style scoped></style>
