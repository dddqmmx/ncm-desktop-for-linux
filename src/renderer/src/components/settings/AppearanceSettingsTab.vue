<script setup lang="ts">
import { storeToRefs } from 'pinia'
import SettingGroup from '@renderer/components/SettingGroup.vue'
import SettingRow from '@renderer/components/SettingRow.vue'
import SegmentedSlider from '@renderer/components/SegmentedSlider.vue'
import { useConfigStore } from '@renderer/stores/configStore'

const configStore = useConfigStore()
const { theme, acrylic, accentColor } = storeToRefs(configStore)
</script>

<template>
  <div class="settings-tab">
    <SettingGroup title="色彩模式" no-card>
      <SegmentedSlider
        v-model="theme"
        :options="[
          { label: '浅色模式', value: 'light' },
          { label: '深色模式', value: 'dark' },
          { label: '跟随系统', value: 'adaptive' }
        ]"
      />
    </SettingGroup>

    <SettingGroup title="视觉效果">
      <SettingRow title="亚克力 / 云母效果" description="启用窗口半透明磨砂背景">
        <input v-model="acrylic" type="checkbox" class="modern-switch" />
      </SettingRow>
      <SettingRow title="强调色" description="用于后续主题扩展和重点控件着色">
        <input v-model="accentColor" type="color" class="color-picker" />
      </SettingRow>
    </SettingGroup>
  </div>
</template>
