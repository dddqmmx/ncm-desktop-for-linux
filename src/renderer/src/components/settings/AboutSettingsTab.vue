<script setup lang="ts">
import { onMounted, ref } from 'vue'
import appIcon from '@renderer/assets/icon.png'
import SettingGroup from '@renderer/components/settings/SettingGroup.vue'
import SettingRow from '@renderer/components/settings/SettingRow.vue'
import { AppInfo } from '@renderer/types/uiService';

let appInfo: AppInfo = {
  name: '',
  version: ''
}

const getAppInfo = async (): Promise<void> => {
  appInfo = await window.api.get_app_info()
  console.log(appInfo)
}

onMounted(async (): Promise<void> => {
  getAppInfo()
})

const checkingUpdate = ref(false)

const checkUpdate = (): void => {
  checkingUpdate.value = true

  setTimeout(() => {
    checkingUpdate.value = false
  }, 2000)
}
</script>

<template>
  <div class="settings-tab">
    <div class="settings-about-hero">
      <div class="settings-app-logo-box">
        <img class="settings-logo-img" :src="appIcon" :alt="appInfo.name" />
      </div>
      <h3 class="settings-app-name">{{ appInfo.name }}</h3>
      <p class="settings-app-version">Version {{ appInfo.version }}</p>
    </div>

    <SettingGroup title="">
      <SettingRow title="检查更新">
        <button class="settings-action-btn" :disabled="checkingUpdate" @click="checkUpdate">
          {{ checkingUpdate ? '检查中...' : '检查更新' }}
        </button>
      </SettingRow>
    </SettingGroup>
  </div>
</template>
