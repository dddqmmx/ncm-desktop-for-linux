<script setup lang="ts">
import { onMounted } from 'vue'
import { storeToRefs } from 'pinia'
import SettingGroup from '@renderer/components/SettingGroup.vue'
import SettingRow from '@renderer/components/SettingRow.vue'
import SegmentedSlider from '@renderer/components/SegmentedSlider.vue'
import { useConfigStore } from '@renderer/stores/configStore'
import { type AudioDeviceInfo } from '@renderer/types/audio'

const configStore = useConfigStore()
const {
  soundQuality,
  audioEngine,
  outputDeviceId,
  exclusiveMode,
  outputDevices,
  currentOutputDevice,
  isLoadingOutputDevices,
  isSwitchingOutputDevice,
  outputDeviceError
} = storeToRefs(configStore)

const formatDeviceLabel = (device: AudioDeviceInfo): string => {
  const flags: string[] = []

  if (device.isDefault) {
    flags.push('系统默认')
  }

  if (device.isCurrent) {
    flags.push('当前')
  }

  return flags.length > 0 ? `${device.name} (${flags.join(' / ')})` : device.name
}

const handleOutputDeviceChange = async (event: Event): Promise<void> => {
  const target = event.target as HTMLSelectElement
  await configStore.setOutputDevice(target.value)
}

const refreshDevices = async (): Promise<void> => {
  await configStore.refreshOutputDevices()
}

onMounted(() => {
  void refreshDevices()
})
</script>

<template>
  <div class="settings-tab">
    <SettingGroup title="音频质量">
      <SettingRow title="默认播放音质" description="选择流媒体播放或下载的默认音质级别">
        <select v-model="soundQuality" class="modern-select">
          <option value="standard">标准 (128kbps)</option>
          <option value="exhigh">极高 (320kbps)</option>
          <option value="lossless">无损 (最高48khz, 16bit)</option>
          <option value="hires">Hi-Res (最高192khz, 24bit)</option>
          <option value="jyeffect">高清臻音 (96khz, 24bit)</option>
          <option value="sky">沉浸环绕声 (最高5.1声道)</option>
          <option value="jymaster">超清母带 (192khz, 24bit)</option>
        </select>
      </SettingRow>
    </SettingGroup>

    <SettingGroup
      title="输出架构"
      tip="Native 提供高性能原生输出；WebAPI 适合兼容性场景。当前设备切换由 Native 输出链路生效。"
      no-card
    >
      <SegmentedSlider
        v-model="audioEngine"
        :options="[
          { label: 'Native', value: 'native' },
          { label: 'WebAPI', value: 'webapi' },
          { label: 'Auto', value: 'auto' }
        ]"
      />
    </SettingGroup>

    <SettingGroup title="设备选择">
      <SettingRow title="指定输出设备" description="切换后会立即调用原生播放器切到对应设备">
        <div class="settings-device-picker">
          <select
            :value="outputDeviceId"
            class="modern-select"
            :disabled="isLoadingOutputDevices || isSwitchingOutputDevice"
            @change="handleOutputDeviceChange"
          >
            <option value="default">系统默认输出</option>
            <option v-for="device in outputDevices" :key="device.id" :value="device.id">
              {{ formatDeviceLabel(device) }}
            </option>
          </select>

          <button
            class="settings-inline-action-btn"
            :disabled="isLoadingOutputDevices || isSwitchingOutputDevice"
            @click="refreshDevices"
          >
            {{
              isLoadingOutputDevices
                ? '刷新中...'
                : isSwitchingOutputDevice
                  ? '切换中...'
                  : '刷新设备'
            }}
          </button>
        </div>
      </SettingRow>

      <SettingRow title="当前设备" description="显示原生播放器当前正在使用的输出端点">
        <span class="settings-status" :class="{ error: outputDeviceError }">
          {{
            outputDeviceError ||
            currentOutputDevice?.name ||
            (isLoadingOutputDevices ? '正在读取设备列表...' : '系统默认输出')
          }}
        </span>
      </SettingRow>

      <SettingRow title="独占输出模式" description="保留配置项，后续可接入更底层的硬件独占逻辑">
        <input v-model="exclusiveMode" type="checkbox" class="modern-switch" />
      </SettingRow>
    </SettingGroup>
  </div>
</template>
