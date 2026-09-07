import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { SoundQualityType } from '@renderer/types/song'
import { AudioDeviceInfo, isSameAudioDevice } from '@renderer/types/audio'
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
  // 偏好只存用户的原始选择；“BitPerfect 隐含独占/禁用软件音量”在每次使用处
  // （outputOptionsFor / canAdjustVolume）派生，不回写。开过一次 BitPerfect 就把
  // 偏好永久覆盖成 true/false 的话，用户关掉 BitPerfect 后还得重新发现并
  // 手动打开自己原先的选择。
  const strictBitPerfect = ref(initialSettings.strictBitPerfect)
  const exclusiveMode = ref(initialSettings.exclusiveMode)
  const softwareVolume = ref(initialSettings.softwareVolume)
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
  }

  const setExclusiveMode = (enabled: boolean): void => {
    exclusiveMode.value = enabled
  }

  const setStrictBitPerfect = (enabled: boolean): void => {
    strictBitPerfect.value = enabled
  }

  const setSoftwareVolume = (enabled: boolean): void => {
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
   * WebAPI 引擎没有独占/BitPerfect 概念，偏好即使开着也不生效。这里只决定
   * “此刻是否生效”，不动用户存下来的偏好：设备被拔掉临时回退到系统默认、
   * 之后重新选回硬件端点（或换回 Native 引擎）时自动恢复。
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
   * 应用级软件音量此刻是否可调（偏好层权威）：用户开了“软件音量”开关即可调，
   * 严格 BitPerfect（强制 1.0 直通）下不可调。WebAPI 引擎没有 BitPerfect 概念，
   * <audio>.volume 永远可调。播放链路的增益计算、播放条音量入口、设置页滑块
   * 禁用态都从这里取。
   *
   * 注意两层状态：这里反映“用户意图”（实时）；“当前会话实际可调”以后端回读的
   * volumeLocked 为准（BitPerfect 开播即冻结，改开关要下一首生效）——播放条/
   * 全屏的滑块同时看两层，锁定时禁用滑块而不是藏掉。
   */
  const canAdjustVolume = computed(
    () => softwareVolume.value && (audioEngine.value === 'webapi' || !strictBitPerfect.value)
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

      // 写法迁移：持久化的 id 与列表条目只是同一端点的不同写法（历史数字
      // 卡号 vs 现在的字符卡名规范写法）时，把存储身份对齐到条目的规范
      // 写法——数字卡号在 USB 拔插后可能漂移，插回一次旧 id 就永久失效；
      // 字符卡名按卡身份解析，跨拔插稳定。只改写法、不改用户选的是哪个
      // 端点（经别名表确认是同一设备才回写）。
      const configuredDevice = normalizedDevices.find((device) =>
        isSameAudioDevice(device, outputDeviceId.value)
      )
      if (
        configuredDevice &&
        configuredDevice.id !== outputDeviceId.value &&
        outputDeviceId.value !== DEFAULT_OUTPUT_DEVICE_ID
      ) {
        outputDeviceId.value = configuredDevice.id
      }

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
    let targetDeviceId = outputDeviceId.value || DEFAULT_OUTPUT_DEVICE_ID
    const devices = await refreshOutputDevices()

    if (isOutputDeviceActive(devices, targetDeviceId)) {
      // refreshOutputDevices 里的写法迁移可能已把存储身份对齐到条目的规范
      // 写法；返回值用同一条目的写法，下游（选项计算/切换）才不会拿着旧
      // 写法再次匹配。
      if (targetDeviceId !== DEFAULT_OUTPUT_DEVICE_ID) {
        const targetEntry = devices.find((device) => isSameAudioDevice(device, targetDeviceId))
        if (targetEntry) {
          targetDeviceId = targetEntry.id
        }
      }
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
