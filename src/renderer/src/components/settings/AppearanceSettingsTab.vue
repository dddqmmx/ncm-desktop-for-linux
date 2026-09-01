<script setup lang="ts">
import { storeToRefs } from 'pinia'
import SettingGroup from '@renderer/components/settings/SettingGroup.vue'
import SettingRow from '@renderer/components/settings/SettingRow.vue'
import SegmentedSlider from '@renderer/components/common/SegmentedSlider.vue'
import { useConfigStore } from '@renderer/stores/configStore'

const configStore = useConfigStore()
const { theme, acrylic, accentColor, fullscreenShowFavorite, fullscreenShowPlayMode } =
  storeToRefs(configStore)
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

    <SettingGroup title="全屏播放页">
      <SettingRow title="显示喜欢按钮" description="在全屏播放页展示喜欢 / 取消喜欢按钮">
        <input v-model="fullscreenShowFavorite" type="checkbox" class="modern-switch" />
      </SettingRow>
      <SettingRow title="显示播放顺序按钮" description="在全屏播放页展示切换播放顺序按钮">
        <input v-model="fullscreenShowPlayMode" type="checkbox" class="modern-switch" />
      </SettingRow>
    </SettingGroup>
  </div>
</template>
