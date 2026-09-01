import { defineStore } from 'pinia'
import { ref, computed, watch } from 'vue'
import { SoundQualityType } from '@renderer/types/song'
import { AudioDeviceInfo } from '@renderer/types/audio'
import { AudioEngineType, DEFAULT_OUTPUT_DEVICE_ID } from './types'
import {
  loadSettings,
  normalizeVolumePercent,
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
  const strictBitPerfect = ref(
    initialSettings.audioEngine === 'webapi' ? false : initialSettings.strictBitPerfect
  )
  const exclusiveMode = ref(
    initialSettings.audioEngine === 'webapi'
      ? false
      : initialSettings.strictBitPerfect || initialSettings.exclusiveMode
  )
  const softwareVolume = ref(
    initialSettings.audioEngine === 'webapi' || !initialSettings.strictBitPerfect
      ? initialSettings.softwareVolume
      : false
  )
  const volumePercent = ref(initialSettings.volumePercent)

  const outputDevices = ref<AudioDeviceInfo[]>([])
  const currentOutputDevice = computed(() => {
    return outputDevices.value.find((device) => device.isCurrent) ?? null
  })
  const isLoadingOutputDevices = ref(false)
  const isSwitchingOutputDevice = ref(false)
  const outputDeviceError = ref('')

  const setAudioEngine = (engine: AudioEngineType): void => {
    audioEngine.value = engine
    if (engine === 'webapi') {
      strictBitPerfect.value = false
      exclusiveMode.value = false
    }
  }

  const setExclusiveMode = (enabled: boolean): void => {
    if (audioEngine.value === 'webapi' || strictBitPerfect.value) return
    exclusiveMode.value = enabled
  }

  const setStrictBitPerfect = (enabled: boolean): void => {
    if (audioEngine.value === 'webapi') return

    strictBitPerfect.value = enabled
    if (enabled) {
      exclusiveMode.value = true
      softwareVolume.value = false
    }
  }

  const setSoftwareVolume = (enabled: boolean): void => {
    if (enabled && strictBitPerfect.value) return
    softwareVolume.value = enabled
  }

  const setVolumePercent = (value: number): void => {
    volumePercent.value = normalizeVolumePercent(value)
  }

  /**
   * 指定输出端点下真正会生效的独占 / BitPerfect 选项。
   *
   * “系统默认输出”是混音服务器的虚拟端点：独占要占住一整张真实声卡，BitPerfect
   * 更要求 hw: 直通，两者在它上面都无从谈起——原生侧会以“不是真实硬件设备”
   * 拒绝播放。把这层判断放在下发之前，选“系统默认输出”才不会被自己的开关堵死
   * （曾经的表现：播着歌切系统默认输出必失败并回滚，用户锁死在 DAC 上）。
   *
   * 注意这里只决定“此刻是否生效”，不动用户存下来的偏好：设备被拔掉临时回退到
   * 系统默认、之后重新选回硬件端点时，独占/BitPerfect 会自动恢复。
   */
  const outputOptionsFor = (
    deviceId: string
  ): { strictBitPerfect: boolean; exclusive: boolean } => {
    if (audioEngine.value === 'webapi' || deviceId === DEFAULT_OUTPUT_DEVICE_ID) {
      return { strictBitPerfect: false, exclusive: false }
    }

    return {
      strictBitPerfect: strictBitPerfect.value,
      // BitPerfect 隐含独占。
      exclusive: strictBitPerfect.value || exclusiveMode.value
    }
  }

  /** 当前选中的端点是否谈得上独占（虚拟的“系统默认输出”谈不上）。 */
  const supportsExclusiveOutput = computed(
    () => audioEngine.value !== 'webapi' && outputDeviceId.value !== DEFAULT_OUTPUT_DEVICE_ID
  )

  /**
   * 应用级软件音量此刻是否可调（唯一权威定义）：
   * 关闭“软件音量”或开启严格 BitPerfect（强制 1.0 直通）时不可调。
   * 播放链路的增益计算、播放条音量入口、设置页滑块禁用态都从这里取。
   */
  const canAdjustVolume = computed(() => softwareVolume.value && !strictBitPerfect.value)

  watch(
    [audioEngine, strictBitPerfect, exclusiveMode, softwareVolume],
    () => {
      if (audioEngine.value === 'webapi') {
        strictBitPerfect.value = false
        exclusiveMode.value = false
        return
      }

      if (!strictBitPerfect.value) return

      exclusiveMode.value = true
      softwareVolume.value = false
    },
    { flush: 'sync' }
  )

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
      // 选项按“切过去之后”的端点算，而不是当前端点：切到系统默认输出时必须把
      // 独占/BitPerfect 摘掉，否则原生侧重放会话时会拒绝，切换失败并回滚。
      const targetOptions = outputOptionsFor(deviceId)
      await window.api.switch_output_device(
        deviceId === DEFAULT_OUTPUT_DEVICE_ID ? undefined : deviceId,
        targetOptions.strictBitPerfect,
        targetOptions.exclusive
      )

      const nextOutputDeviceName = resolveConfiguredOutputDeviceName(
        outputDevices.value,
        deviceId,
        deviceId === outputDeviceId.value ? outputDeviceName.value : ''
      )

      if (persistSelection) {
        outputDeviceId.value = deviceId
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
    strictBitPerfect,
    softwareVolume,
    volumePercent,
    outputDevices,
    currentOutputDevice,
    isLoadingOutputDevices,
    isSwitchingOutputDevice,
    outputDeviceError,
    supportsExclusiveOutput,
    canAdjustVolume,
    outputOptionsFor,
    refreshOutputDevices,
    applyOutputDevice,
    ensureConfiguredOutputDevice,
    setAudioEngine,
    setExclusiveMode,
    setStrictBitPerfect,
    setSoftwareVolume,
    setVolumePercent
  }
})
