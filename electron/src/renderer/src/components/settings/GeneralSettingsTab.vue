<script setup lang="ts">
import { storeToRefs } from 'pinia'
import SettingGroup from '@renderer/components/settings/SettingGroup.vue'
import SettingRow from '@renderer/components/settings/SettingRow.vue'
import { useConfigStore } from '@renderer/stores/configStore'

const configStore = useConfigStore()
const { autoLaunch, trayMinimize } = storeToRefs(configStore)
</script>

<template>
  <div class="settings-tab">
    <SettingGroup title="启动与运行">
      <SettingRow title="开机自启动" description="在登录系统时自动启动音乐播放器">
        <input v-model="autoLaunch" type="checkbox" class="modern-switch" />
      </SettingRow>
      <SettingRow title="最小化至托盘" description="关闭主窗口时应用将继续在后台运行">
        <input v-model="trayMinimize" type="checkbox" class="modern-switch" />
      </SettingRow>
    </SettingGroup>
  </div>
</template>
