import { type SoundQualityType } from '@renderer/types/song'
import { type AudioDeviceInfo } from '@renderer/types/audio'
import { defineStore } from 'pinia'
import { computed, ref, watch } from 'vue'

export type AudioEngineType = 'native' | 'webapi' | 'auto'
export type ThemeMode = 'light' | 'dark' | 'adaptive'

interface PersistedSettings {
  soundQuality: SoundQualityType
  autoLaunch: boolean
  trayMinimize: boolean
  audioEngine: AudioEngineType
  outputDeviceId: string
  exclusiveMode: boolean
  theme: ThemeMode
  acrylic: boolean
  accentColor: string
  libPaths: string[]
}

const STORAGE_KEY = 'app_settings'
const LEGACY_SOUND_QUALITY_KEY = 'sound_quality'
const DEFAULT_OUTPUT_DEVICE_ID = 'default'

const SOUND_QUALITIES: SoundQualityType[] = [
  'standard',
  'exhigh',
  'lossless',
  'hires',
  'jyeffect',
  'sky',
  'jymaster'
]
const AUDIO_ENGINES: AudioEngineType[] = ['native', 'webapi', 'auto']
const THEMES: ThemeMode[] = ['light', 'dark', 'adaptive']

const DEFAULT_SETTINGS: PersistedSettings = {
  soundQuality: 'hires',
  autoLaunch: true,
  trayMinimize: true,
  audioEngine: 'native',
  outputDeviceId: DEFAULT_OUTPUT_DEVICE_ID,
  exclusiveMode: false,
  theme: 'adaptive',
  acrylic: true,
  accentColor: '#6366f1',
  libPaths: []
}

function isSoundQuality(value: unknown): value is SoundQualityType {
  return SOUND_QUALITIES.includes(value as SoundQualityType)
}

function isAudioEngine(value: unknown): value is AudioEngineType {
  return AUDIO_ENGINES.includes(value as AudioEngineType)
}

function isThemeMode(value: unknown): value is ThemeMode {
  return THEMES.includes(value as ThemeMode)
}

function getLegacySoundQuality(): SoundQualityType {
  const saved = localStorage.getItem(LEGACY_SOUND_QUALITY_KEY)
  return isSoundQuality(saved) ? saved : DEFAULT_SETTINGS.soundQuality
}

function normalizeLibraryPaths(paths: unknown): string[] {
  if (!Array.isArray(paths)) {
    return [...DEFAULT_SETTINGS.libPaths]
  }

  const uniquePaths = new Set(
    paths
      .filter((path): path is string => typeof path === 'string')
      .map((path) => path.trim())
      .filter(Boolean)
  )

  return Array.from(uniquePaths)
}

function loadSettings(): PersistedSettings {
  const fallbackSettings: PersistedSettings = {
    ...DEFAULT_SETTINGS,
    soundQuality: getLegacySoundQuality()
  }

  const rawSettings = localStorage.getItem(STORAGE_KEY)
  if (!rawSettings) {
    return fallbackSettings
  }

  try {
    const parsed = JSON.parse(rawSettings) as Partial<PersistedSettings>

    return {
      soundQuality: isSoundQuality(parsed.soundQuality)
        ? parsed.soundQuality
        : fallbackSettings.soundQuality,
      autoLaunch:
        typeof parsed.autoLaunch === 'boolean' ? parsed.autoLaunch : fallbackSettings.autoLaunch,
      trayMinimize:
        typeof parsed.trayMinimize === 'boolean'
          ? parsed.trayMinimize
          : fallbackSettings.trayMinimize,
      audioEngine: isAudioEngine(parsed.audioEngine)
        ? parsed.audioEngine
        : fallbackSettings.audioEngine,
      outputDeviceId:
        typeof parsed.outputDeviceId === 'string' && parsed.outputDeviceId.trim().length > 0
          ? parsed.outputDeviceId
          : fallbackSettings.outputDeviceId,
      exclusiveMode:
        typeof parsed.exclusiveMode === 'boolean'
          ? parsed.exclusiveMode
          : fallbackSettings.exclusiveMode,
      theme: isThemeMode(parsed.theme) ? parsed.theme : fallbackSettings.theme,
      acrylic: typeof parsed.acrylic === 'boolean' ? parsed.acrylic : fallbackSettings.acrylic,
      accentColor:
        typeof parsed.accentColor === 'string' && parsed.accentColor.trim().length > 0
          ? parsed.accentColor
          : fallbackSettings.accentColor,
      libPaths: normalizeLibraryPaths(parsed.libPaths)
    }
  } catch (error) {
    console.warn('读取设置失败，使用默认配置。', error)
    return fallbackSettings
  }
}

function persistSettings(settings: PersistedSettings): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(settings))
  localStorage.setItem(LEGACY_SOUND_QUALITY_KEY, settings.soundQuality)
}

type RawAudioDeviceInfo = {
  id?: unknown
  name?: unknown
  isDefault?: unknown
  isCurrent?: unknown
  is_default?: unknown
  is_current?: unknown
}

function normalizeOutputDevice(device: RawAudioDeviceInfo): AudioDeviceInfo | null {
  if (typeof device.id !== 'string' || typeof device.name !== 'string') {
    return null
  }

  const isDefault =
    typeof device.isDefault === 'boolean'
      ? device.isDefault
      : typeof device.is_default === 'boolean'
        ? device.is_default
        : null
  const isCurrent =
    typeof device.isCurrent === 'boolean'
      ? device.isCurrent
      : typeof device.is_current === 'boolean'
        ? device.is_current
        : null

  if (isDefault === null || isCurrent === null) {
    return null
  }

  return {
    id: device.id,
    name: device.name,
    isDefault,
    isCurrent
  }
}

function normalizeOutputDevices(devices: RawAudioDeviceInfo[]): AudioDeviceInfo[] {
  return devices
    .map(normalizeOutputDevice)
    .filter((device): device is AudioDeviceInfo => device !== null)
    .sort((left, right) => {
      if (left.isCurrent !== right.isCurrent) {
        return Number(right.isCurrent) - Number(left.isCurrent)
      }

      if (left.isDefault !== right.isDefault) {
        return Number(right.isDefault) - Number(left.isDefault)
      }

      return left.name.localeCompare(right.name, 'zh-CN')
    })
}

function withConfiguredOutputDevice(
  devices: AudioDeviceInfo[],
  configuredDeviceId: string
): AudioDeviceInfo[] {
  if (
    configuredDeviceId === DEFAULT_OUTPUT_DEVICE_ID ||
    devices.some((device) => device.id === configuredDeviceId)
  ) {
    return devices
  }

  return [
    {
      id: configuredDeviceId,
      name: `已配置设备（当前不可用） [${configuredDeviceId}]`,
      isDefault: false,
      isCurrent: false
    },
    ...devices
  ]
}

function isOutputDeviceActive(devices: AudioDeviceInfo[], targetDeviceId: string): boolean {
  const currentDevice = devices.find((device) => device.isCurrent)

  if (!currentDevice) {
    return false
  }

  if (targetDeviceId === DEFAULT_OUTPUT_DEVICE_ID) {
    return currentDevice.isDefault
  }

  return currentDevice.id === targetDeviceId
}

export const useConfigStore = defineStore('config', () => {
  const initialSettings = loadSettings()

  const soundQuality = ref<SoundQualityType>(initialSettings.soundQuality)
  const autoLaunch = ref(initialSettings.autoLaunch)
  const trayMinimize = ref(initialSettings.trayMinimize)
  const audioEngine = ref<AudioEngineType>(initialSettings.audioEngine)
  const outputDeviceId = ref(initialSettings.outputDeviceId)
  const exclusiveMode = ref(initialSettings.exclusiveMode)
  const theme = ref<ThemeMode>(initialSettings.theme)
  const acrylic = ref(initialSettings.acrylic)
  const accentColor = ref(initialSettings.accentColor)
  const libPaths = ref<string[]>(initialSettings.libPaths)

  const outputDevices = ref<AudioDeviceInfo[]>([])
  const currentOutputDevice = computed(() => {
    return outputDevices.value.find((device) => device.isCurrent) ?? null
  })
  const isLoadingOutputDevices = ref(false)
  const isSwitchingOutputDevice = ref(false)
  const outputDeviceError = ref('')

  const snapshotSettings = (): PersistedSettings => ({
    soundQuality: soundQuality.value,
    autoLaunch: autoLaunch.value,
    trayMinimize: trayMinimize.value,
    audioEngine: audioEngine.value,
    outputDeviceId: outputDeviceId.value,
    exclusiveMode: exclusiveMode.value,
    theme: theme.value,
    acrylic: acrylic.value,
    accentColor: accentColor.value,
    libPaths: [...libPaths.value]
  })

  watch(
    [
      soundQuality,
      autoLaunch,
      trayMinimize,
      audioEngine,
      outputDeviceId,
      exclusiveMode,
      theme,
      acrylic,
      accentColor,
      libPaths
    ],
    () => {
      persistSettings(snapshotSettings())
    },
    { deep: true }
  )

  const refreshOutputDevices = async (): Promise<AudioDeviceInfo[]> => {
    isLoadingOutputDevices.value = true
    outputDeviceError.value = ''

    try {
      const devices = withConfiguredOutputDevice(
        normalizeOutputDevices(await window.api.get_output_devices()),
        outputDeviceId.value
      )
      outputDevices.value = devices
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

      if (persistSelection) {
        outputDeviceId.value = deviceId
      }

      if (refreshAfterSwitch) {
        await refreshOutputDevices()
      }

      return true
    } catch (error) {
      outputDeviceError.value = '切换音频输出设备失败，请重试。'
      console.error('切换音频设备失败', error)

      if (refreshAfterSwitch) {
        await refreshOutputDevices()
      }

      return false
    } finally {
      isSwitchingOutputDevice.value = false
    }
  }

  let initializePromise: Promise<void> | null = null

  const initialize = async (): Promise<void> => {
    if (initializePromise) {
      return initializePromise
    }

    initializePromise = (async () => {
      await ensureConfiguredOutputDevice()
    })().catch((error) => {
      initializePromise = null
      console.error('初始化设置失败', error)
      throw error
    })

    return initializePromise
  }

  const setSoundQuality = (quality: SoundQualityType): void => {
    soundQuality.value = quality
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

  const setOutputDevice = async (deviceId: string): Promise<boolean> => {
    const targetDeviceId = deviceId || DEFAULT_OUTPUT_DEVICE_ID
    return applyOutputDevice(targetDeviceId, true)
  }

  const addLibraryPath = (path: string): boolean => {
    const normalizedPath = path.trim()

    if (!normalizedPath || libPaths.value.includes(normalizedPath)) {
      return false
    }

    libPaths.value = [...libPaths.value, normalizedPath]
    return true
  }

  const removeLibraryPath = (path: string): void => {
    libPaths.value = libPaths.value.filter((item) => item !== path)
  }

  return {
    soundQuality,
    autoLaunch,
    trayMinimize,
    audioEngine,
    outputDeviceId,
    exclusiveMode,
    theme,
    acrylic,
    accentColor,
    libPaths,
    outputDevices,
    currentOutputDevice,
    isLoadingOutputDevices,
    isSwitchingOutputDevice,
    outputDeviceError,
    initialize,
    refreshOutputDevices,
    ensureConfiguredOutputDevice,
    setSoundQuality,
    setOutputDevice,
    addLibraryPath,
    removeLibraryPath
  }
})
