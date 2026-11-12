<script setup lang="ts">
import { storeToRefs } from 'pinia'
import SettingGroup from '@renderer/components/settings/SettingGroup.vue'
import SettingRow from '@renderer/components/settings/SettingRow.vue'
import { useConfigStore } from '@renderer/stores/configStore'

const configStore = useConfigStore()
const { lyricDebug } = storeToRefs(configStore)
</script>

<template>
  <div class="settings-tab">
    <SettingGroup title="调试">
      <SettingRow title="歌词 debug" description="显示歌词同步调试面板并输出相关调试日志">
        <input v-model="lyricDebug" type="checkbox" class="modern-switch" />
      </SettingRow>
    </SettingGroup>
  </div>
</template>
