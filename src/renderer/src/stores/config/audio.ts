import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { SoundQualityType } from '@renderer/types/song'
import { AudioDeviceInfo } from '@renderer/types/audio'
import { AudioEngineType, DEFAULT_OUTPUT_DEVICE_ID } from './types'
import {
  loadSettings,
  normalizeOutputDevices,
  withConfiguredOutputDevice,
  resolveConfiguredOutputDeviceName,
  isMissingOutputDeviceError,
  isOutputDeviceActive
} from './utils'

export const useAudioConfigStore = defineStore('audioConfig', () => {
  const initialSettings = loadSettings()

  const soundQuality = ref<SoundQualityType>(initialSettings.soundQuality)
  const audioEngine = ref<AudioEngineType>(initialSettings.audioEngine)
  const outputDeviceId = ref(initialSettings.outputDeviceId)
  const outputDeviceName = ref(initialSettings.outputDeviceName)
  const exclusiveMode = ref(initialSettings.exclusiveMode)

  const outputDevices = ref<AudioDeviceInfo[]>([])
  const currentOutputDevice = computed(() => {
    return outputDevices.value.find((device) => device.isCurrent) ?? null
  })
  const isLoadingOutputDevices = ref(false)
  const isSwitchingOutputDevice = ref(false)
  const outputDeviceError = ref('')

  const refreshOutputDevices = async (): Promise<AudioDeviceInfo[]> => {
    isLoadingOutputDevices.value = true
    outputDeviceError.value = ''

    try {
      const normalizedDevices = normalizeOutputDevices(await window.api.get_output_devices())
      const devices = withConfiguredOutputDevice(
        normalizedDevices,
        outputDeviceId.value,
        outputDeviceName.value
      )
      outputDevices.value = devices
      outputDeviceName.value = resolveConfiguredOutputDeviceName(
        normalizedDevices,
        outputDeviceId.value,
        outputDeviceName.value
      )
      return devices
    } catch (error) {
      outputDeviceError.value = '读取音频设备失败，请稍后重试。'
      console.error('获取音频设备失败', error)
      return outputDevices.value
    } finally {
      isLoadingOutputDevices.value = false
    }
  }

  const applyOutputDevice = async (
    deviceId = DEFAULT_OUTPUT_DEVICE_ID,
    refreshAfterSwitch = true,
    persistSelection = true
  ): Promise<boolean> => {
    isSwitchingOutputDevice.value = true
    outputDeviceError.value = ''

    try {
      await window.api.switch_output_device(
        deviceId === DEFAULT_OUTPUT_DEVICE_ID ? undefined : deviceId
      )

      const nextOutputDeviceName = resolveConfiguredOutputDeviceName(
        outputDevices.value,
        deviceId,
        deviceId === outputDeviceId.value ? outputDeviceName.value : ''
      )

      if (persistSelection) {
        outputDeviceId.value = deviceId
        outputDeviceName.value = nextOutputDeviceName
      } else {
        // Even if not persisting (e.g. temporary fallback), update the name for UI feedback
        outputDeviceName.value = nextOutputDeviceName
      }

      if (refreshAfterSwitch) {
        await refreshOutputDevices()
      }

      return true
    } catch (error) {
      outputDeviceError.value = isMissingOutputDeviceError(error)
        ? '已配置的音频设备当前不可用，当前播放会话将回退到系统默认输出。'
        : '切换音频输出设备失败，请重试。'

      if (refreshAfterSwitch) {
        await refreshOutputDevices()
      }

      return false
    } finally {
      isSwitchingOutputDevice.value = false
    }
  }

  const ensureConfiguredOutputDevice = async (): Promise<string> => {
    const targetDeviceId = outputDeviceId.value || DEFAULT_OUTPUT_DEVICE_ID
    const devices = await refreshOutputDevices()

    if (isOutputDeviceActive(devices, targetDeviceId)) {
      return targetDeviceId
    }

    const switched = await applyOutputDevice(targetDeviceId, false, false)
    await refreshOutputDevices()

    if (switched || targetDeviceId === DEFAULT_OUTPUT_DEVICE_ID) {
      return targetDeviceId
    }

    await applyOutputDevice(DEFAULT_OUTPUT_DEVICE_ID, false, false)
    await refreshOutputDevices()
    return DEFAULT_OUTPUT_DEVICE_ID
  }

  return {
    soundQuality,
    audioEngine,
    outputDeviceId,
    outputDeviceName,
    exclusiveMode,
    outputDevices,
    currentOutputDevice,
    isLoadingOutputDevices,
    isSwitchingOutputDevice,
    outputDeviceError,
    refreshOutputDevices,
    applyOutputDevice,
    ensureConfiguredOutputDevice
  }
})
