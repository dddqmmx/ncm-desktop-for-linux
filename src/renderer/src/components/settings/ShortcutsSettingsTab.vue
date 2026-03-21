<script setup lang="ts">
import SettingGroup from '@renderer/components/SettingGroup.vue'
</script>

<template>
  <div class="settings-tab">
    <SettingGroup title="快捷键" no-card>
      <div class="settings-placeholder-card">
        快捷键配置还没有接入实际绑定逻辑。现在先把这个分区单独拆出来，后续可以在这里统一管理播放/暂停、上一首、下一首等全局快捷键。
      </div>
    </SettingGroup>
  </div>
</template>
