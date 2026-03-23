import { type SoundQualityType } from '@renderer/types/song'
import { type AudioDeviceInfo } from '@renderer/types/audio'
import { type CacheStats } from '@renderer/types/cache'
import { clearResolvedMediaUrlCache } from '@renderer/utils/cache'
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
  outputDeviceName: string
  exclusiveMode: boolean
  theme: ThemeMode
  acrylic: boolean
  accentColor: string
  libPaths: string[]
  cacheLimitMb: number
  songCacheAheadSecs: number
}

const STORAGE_KEY = 'app_settings'
const LEGACY_SOUND_QUALITY_KEY = 'sound_quality'
const DEFAULT_OUTPUT_DEVICE_ID = 'default'
const DEFAULT_OUTPUT_DEVICE_NAME = '系统默认输出'
const DEFAULT_CACHE_LIMIT_MB = 512
const MIN_CACHE_LIMIT_MB = 128
const MAX_CACHE_LIMIT_MB = 8192
const DEFAULT_SONG_CACHE_AHEAD_SECS = 30
const MIN_SONG_CACHE_AHEAD_SECS = 10
const MAX_SONG_CACHE_AHEAD_SECS = 300

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
  outputDeviceName: DEFAULT_OUTPUT_DEVICE_NAME,
  exclusiveMode: false,
  theme: 'adaptive',
  acrylic: true,
  accentColor: '#6366f1',
  libPaths: [],
  cacheLimitMb: DEFAULT_CACHE_LIMIT_MB,
  songCacheAheadSecs: DEFAULT_SONG_CACHE_AHEAD_SECS
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

function normalizeOutputDeviceId(deviceId: unknown, fallbackDeviceId: string): string {
  if (typeof deviceId !== 'string') {
    return fallbackDeviceId
  }

  const normalizedDeviceId = deviceId.trim()
  return normalizedDeviceId.length > 0 ? normalizedDeviceId : fallbackDeviceId
}

function normalizeOutputDeviceName(deviceName: unknown, deviceId: string): string {
  if (typeof deviceName === 'string' && deviceName.trim().length > 0) {
    return deviceName.trim()
  }

  return deviceId === DEFAULT_OUTPUT_DEVICE_ID ? DEFAULT_OUTPUT_DEVICE_NAME : ''
}

function normalizeCacheLimitMb(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return DEFAULT_SETTINGS.cacheLimitMb
  }

  return Math.min(MAX_CACHE_LIMIT_MB, Math.max(MIN_CACHE_LIMIT_MB, Math.round(value)))
}

function normalizeSongCacheAheadSecs(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return DEFAULT_SETTINGS.songCacheAheadSecs
  }

  return Math.min(
    MAX_SONG_CACHE_AHEAD_SECS,
    Math.max(MIN_SONG_CACHE_AHEAD_SECS, Math.round(value))
  )
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
    const outputDeviceId = normalizeOutputDeviceId(
      parsed.outputDeviceId,
      fallbackSettings.outputDeviceId
    )

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
      outputDeviceId,
      outputDeviceName: normalizeOutputDeviceName(parsed.outputDeviceName, outputDeviceId),
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
      libPaths: normalizeLibraryPaths(parsed.libPaths),
      cacheLimitMb: normalizeCacheLimitMb(parsed.cacheLimitMb),
      songCacheAheadSecs: normalizeSongCacheAheadSecs(parsed.songCacheAheadSecs)
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

function buildUnavailableOutputDeviceName(deviceId: string, deviceName = ''): string {
  const normalizedDeviceName = deviceName.trim()
  return normalizedDeviceName
    ? `${normalizedDeviceName} (当前不可用)`
    : `[${deviceId}] 当前不可用`
}

function withConfiguredOutputDevice(
  devices: AudioDeviceInfo[],
  configuredDeviceId: string,
  configuredDeviceName: string
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
      name: buildUnavailableOutputDeviceName(configuredDeviceId, configuredDeviceName),
      isDefault: false,
      isCurrent: false
    },
    ...devices
  ]
}

function findConfiguredOutputDevice(
  devices: AudioDeviceInfo[],
  configuredDeviceId: string
): AudioDeviceInfo | null {
  if (configuredDeviceId === DEFAULT_OUTPUT_DEVICE_ID) {
    return (
      devices.find((device) => device.id === DEFAULT_OUTPUT_DEVICE_ID) ??
      devices.find((device) => device.isDefault) ??
      null
    )
  }

  return devices.find((device) => device.id === configuredDeviceId) ?? null
}

function resolveConfiguredOutputDeviceName(
  devices: AudioDeviceInfo[],
  configuredDeviceId: string,
  configuredDeviceName: string
): string {
  const configuredDevice = findConfiguredOutputDevice(devices, configuredDeviceId)

  if (configuredDevice) {
    return configuredDevice.name
  }

  if (configuredDeviceId === DEFAULT_OUTPUT_DEVICE_ID) {
    return DEFAULT_OUTPUT_DEVICE_NAME
  }

  return configuredDeviceName.trim()
}

function createEmptyCacheStats(maxSizeBytes = DEFAULT_CACHE_LIMIT_MB * 1024 * 1024): CacheStats {
  return {
    totalBytes: 0,
    maxSizeBytes,
    songBytes: 0,
    songEntries: 0,
    entityBytes: 0,
    entityEntries: 0,
    coverBytes: 0,
    coverEntries: 0,
    lyricBytes: 0,
    lyricEntries: 0
  }
}

type RawCacheStats = Partial<
  Record<
    | 'totalBytes'
    | 'total_bytes'
    | 'maxSizeBytes'
    | 'max_size_bytes'
    | 'songBytes'
    | 'song_bytes'
    | 'songEntries'
    | 'song_entries'
    | 'entityBytes'
    | 'entity_bytes'
    | 'entityEntries'
    | 'entity_entries'
    | 'coverBytes'
    | 'cover_bytes'
    | 'coverEntries'
    | 'cover_entries'
    | 'lyricBytes'
    | 'lyric_bytes'
    | 'lyricEntries'
    | 'lyric_entries',
    unknown
  >
>

function toFiniteNumber(value: unknown, fallback = 0): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

function normalizeCacheStats(
  stats: RawCacheStats | null | undefined,
  fallbackMaxBytes = DEFAULT_CACHE_LIMIT_MB * 1024 * 1024
): CacheStats {
  return {
    totalBytes: toFiniteNumber(stats?.totalBytes ?? stats?.total_bytes),
    maxSizeBytes: toFiniteNumber(stats?.maxSizeBytes ?? stats?.max_size_bytes, fallbackMaxBytes),
    songBytes: toFiniteNumber(stats?.songBytes ?? stats?.song_bytes),
    songEntries: toFiniteNumber(stats?.songEntries ?? stats?.song_entries),
    entityBytes: toFiniteNumber(stats?.entityBytes ?? stats?.entity_bytes),
    entityEntries: toFiniteNumber(stats?.entityEntries ?? stats?.entity_entries),
    coverBytes: toFiniteNumber(stats?.coverBytes ?? stats?.cover_bytes),
    coverEntries: toFiniteNumber(stats?.coverEntries ?? stats?.cover_entries),
    lyricBytes: toFiniteNumber(stats?.lyricBytes ?? stats?.lyric_bytes),
    lyricEntries: toFiniteNumber(stats?.lyricEntries ?? stats?.lyric_entries)
  }
}

function bytesToMegabytes(value: number): number {
  return Math.max(MIN_CACHE_LIMIT_MB, Math.round(value / (1024 * 1024)))
}

function megabytesToBytes(value: number): number {
  return normalizeCacheLimitMb(value) * 1024 * 1024
}

function getErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message
  }

  if (typeof error === 'string') {
    return error
  }

  return ''
}

function isMissingOutputDeviceError(error: unknown): boolean {
  const message = getErrorMessage(error).toLowerCase()
  return message.includes('device not found') || message.includes('temporarily unavailable')
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
  const outputDeviceName = ref(initialSettings.outputDeviceName)
  const exclusiveMode = ref(initialSettings.exclusiveMode)
  const theme = ref<ThemeMode>(initialSettings.theme)
  const acrylic = ref(initialSettings.acrylic)
  const accentColor = ref(initialSettings.accentColor)
  const libPaths = ref<string[]>(initialSettings.libPaths)
  const cacheLimitMb = ref(initialSettings.cacheLimitMb)
  const songCacheAheadSecs = ref(initialSettings.songCacheAheadSecs)

  const outputDevices = ref<AudioDeviceInfo[]>([])
  const currentOutputDevice = computed(() => {
    return outputDevices.value.find((device) => device.isCurrent) ?? null
  })
  const isLoadingOutputDevices = ref(false)
  const isSwitchingOutputDevice = ref(false)
  const outputDeviceError = ref('')
  const cacheStats = ref<CacheStats>(createEmptyCacheStats(megabytesToBytes(cacheLimitMb.value)))
  const isLoadingCacheStats = ref(false)
  const isUpdatingCacheLimit = ref(false)
  const isUpdatingSongCacheAheadSecs = ref(false)
  const isClearingCache = ref(false)
  const cacheError = ref('')

  const snapshotSettings = (): PersistedSettings => ({
    soundQuality: soundQuality.value,
    autoLaunch: autoLaunch.value,
    trayMinimize: trayMinimize.value,
    audioEngine: audioEngine.value,
    outputDeviceId: outputDeviceId.value,
    outputDeviceName: outputDeviceName.value,
    exclusiveMode: exclusiveMode.value,
    theme: theme.value,
    acrylic: acrylic.value,
    accentColor: accentColor.value,
    libPaths: [...libPaths.value],
    cacheLimitMb: cacheLimitMb.value,
    songCacheAheadSecs: songCacheAheadSecs.value
  })

  watch(
    [
      soundQuality,
      autoLaunch,
      trayMinimize,
      audioEngine,
      outputDeviceId,
      outputDeviceName,
      exclusiveMode,
      theme,
      acrylic,
      accentColor,
      libPaths,
      cacheLimitMb,
      songCacheAheadSecs
    ],
    () => {
      persistSettings(snapshotSettings())
    },
    { deep: true }
  )

  const syncConfiguredOutputDeviceName = (devices: AudioDeviceInfo[]): void => {
    outputDeviceName.value = resolveConfiguredOutputDeviceName(
      devices,
      outputDeviceId.value,
      outputDeviceName.value
    )
  }

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
      syncConfiguredOutputDeviceName(normalizedDevices)
      return devices
    } catch (error) {
      outputDeviceError.value = '读取音频设备失败，请稍后重试。'
      console.error('获取音频设备失败', error)
      return outputDevices.value
    } finally {
      isLoadingOutputDevices.value = false
    }
  }

  const refreshCacheStats = async (): Promise<CacheStats> => {
    isLoadingCacheStats.value = true
    cacheError.value = ''

    try {
      const [rawStats, rawSongCacheAheadSecs] = await Promise.all([
        window.api.cache_get_stats(),
        window.api.cache_get_song_cache_ahead_secs()
      ])
      const stats = normalizeCacheStats(rawStats as RawCacheStats, megabytesToBytes(cacheLimitMb.value))
      cacheStats.value = stats
      cacheLimitMb.value = normalizeCacheLimitMb(bytesToMegabytes(stats.maxSizeBytes))
      songCacheAheadSecs.value = normalizeSongCacheAheadSecs(rawSongCacheAheadSecs)
      return stats
    } catch (error) {
      cacheError.value = '读取缓存状态失败，请稍后重试。'
      console.error('读取缓存状态失败', error)
      return cacheStats.value
    } finally {
      isLoadingCacheStats.value = false
    }
  }

  const setCacheLimit = async (nextLimitMb: number): Promise<boolean> => {
    isUpdatingCacheLimit.value = true
    cacheError.value = ''

    const normalizedLimitMb = normalizeCacheLimitMb(nextLimitMb)

    try {
      const stats = normalizeCacheStats(
        (await window.api.cache_set_max_size(megabytesToBytes(normalizedLimitMb))) as RawCacheStats,
        megabytesToBytes(normalizedLimitMb)
      )

      cacheStats.value = stats
      cacheLimitMb.value = normalizeCacheLimitMb(bytesToMegabytes(stats.maxSizeBytes))
      return true
    } catch (error) {
      cacheError.value = '更新缓存上限失败，请重试。'
      console.error('更新缓存上限失败', error)
      await refreshCacheStats()
      return false
    } finally {
      isUpdatingCacheLimit.value = false
    }
  }

  const clearCache = async (): Promise<boolean> => {
    isClearingCache.value = true
    cacheError.value = ''

    try {
      const stats = normalizeCacheStats(
        (await window.api.cache_clear()) as RawCacheStats,
        megabytesToBytes(cacheLimitMb.value)
      )
      clearResolvedMediaUrlCache()
      cacheStats.value = stats
      cacheLimitMb.value = normalizeCacheLimitMb(bytesToMegabytes(stats.maxSizeBytes))
      return true
    } catch (error) {
      cacheError.value = '清理缓存失败，请稍后再试。'
      console.error('清理缓存失败', error)
      return false
    } finally {
      isClearingCache.value = false
    }
  }

  const setSongCacheAheadTime = async (nextSecs: number): Promise<boolean> => {
    isUpdatingSongCacheAheadSecs.value = true
    cacheError.value = ''

    const normalizedSecs = normalizeSongCacheAheadSecs(nextSecs)

    try {
      songCacheAheadSecs.value = normalizeSongCacheAheadSecs(
        await window.api.cache_set_song_cache_ahead_secs(normalizedSecs)
      )
      return true
    } catch (error) {
      cacheError.value = '更新歌曲预缓存时长失败，请重试。'
      console.error('更新歌曲预缓存时长失败', error)
      await refreshCacheStats()
      return false
    } finally {
      isUpdatingSongCacheAheadSecs.value = false
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
        const nextOutputDeviceName = resolveConfiguredOutputDeviceName(
          outputDevices.value,
          deviceId,
          deviceId === outputDeviceId.value ? outputDeviceName.value : ''
        )
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

      if (isMissingOutputDeviceError(error)) {
        console.warn('配置的音频设备当前不可用', error)
      } else {
        console.error('切换音频设备失败', error)
      }

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
      await Promise.all([ensureConfiguredOutputDevice(), refreshCacheStats()])
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
    outputDeviceName,
    exclusiveMode,
    theme,
    acrylic,
    accentColor,
    libPaths,
    cacheLimitMb,
    songCacheAheadSecs,
    cacheStats,
    outputDevices,
    currentOutputDevice,
    isLoadingOutputDevices,
    isSwitchingOutputDevice,
    outputDeviceError,
    isLoadingCacheStats,
    isUpdatingCacheLimit,
    isUpdatingSongCacheAheadSecs,
    isClearingCache,
    cacheError,
    initialize,
    refreshOutputDevices,
    refreshCacheStats,
    ensureConfiguredOutputDevice,
    setSoundQuality,
    setOutputDevice,
    setCacheLimit,
    setSongCacheAheadTime,
    clearCache,
    addLibraryPath,
    removeLibraryPath
  }
})
