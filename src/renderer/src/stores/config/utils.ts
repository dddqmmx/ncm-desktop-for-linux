import { SoundQualityType } from '@renderer/types/song'
import { AudioDeviceInfo } from '@renderer/types/audio'
import { CacheStats } from '@renderer/types/cache'
import {
  AudioEngineType,
  ThemeMode,
  PersistedSettings,
  DEFAULT_SETTINGS,
  STORAGE_KEY,
  LEGACY_SOUND_QUALITY_KEY,
  SOUND_QUALITIES,
  AUDIO_ENGINES,
  THEMES,
  DEFAULT_OUTPUT_DEVICE_ID,
  DEFAULT_OUTPUT_DEVICE_NAME,
  MAX_CACHE_LIMIT_MB,
  MIN_CACHE_LIMIT_MB,
  MAX_SONG_MAX_CACHE_AHEAD_MB,
  MIN_SONG_MAX_CACHE_AHEAD_MB,
  MAX_VOLUME_PERCENT,
  MIN_VOLUME_PERCENT
} from './types'

export function isSoundQuality(value: unknown): value is SoundQualityType {
  return SOUND_QUALITIES.includes(value as SoundQualityType)
}

export function isAudioEngine(value: unknown): value is AudioEngineType {
  return AUDIO_ENGINES.includes(value as AudioEngineType)
}

export function isThemeMode(value: unknown): value is ThemeMode {
  return THEMES.includes(value as ThemeMode)
}

function getLegacySoundQuality(): SoundQualityType {
  const saved = localStorage.getItem(LEGACY_SOUND_QUALITY_KEY)
  return isSoundQuality(saved) ? saved : DEFAULT_SETTINGS.soundQuality
}

export function normalizeLibraryPaths(paths: unknown): string[] {
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

export function normalizeOutputDeviceId(deviceId: unknown, fallbackDeviceId: string): string {
  if (typeof deviceId !== 'string') {
    return fallbackDeviceId
  }

  const normalizedDeviceId = deviceId.trim()
  return normalizedDeviceId.length > 0 ? normalizedDeviceId : fallbackDeviceId
}

export function normalizeOutputDeviceName(deviceName: unknown, deviceId: string): string {
  if (typeof deviceName === 'string' && deviceName.trim().length > 0) {
    return deviceName.trim()
  }

  return deviceId === DEFAULT_OUTPUT_DEVICE_ID ? DEFAULT_OUTPUT_DEVICE_NAME : ''
}

export function normalizeCacheLimitMb(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return DEFAULT_SETTINGS.cacheLimitMb
  }

  return Math.min(MAX_CACHE_LIMIT_MB, Math.max(MIN_CACHE_LIMIT_MB, Math.round(value)))
}

export function normalizeSongMaxCacheAheadMb(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return DEFAULT_SETTINGS.songMaxCacheAheadMb
  }

  return Math.min(
    MAX_SONG_MAX_CACHE_AHEAD_MB,
    Math.max(MIN_SONG_MAX_CACHE_AHEAD_MB, Math.round(value))
  )
}

export function normalizeVolumePercent(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return DEFAULT_SETTINGS.volumePercent
  }

  return Math.min(MAX_VOLUME_PERCENT, Math.max(MIN_VOLUME_PERCENT, Math.round(value)))
}

export function loadSettings(): PersistedSettings {
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
      strictBitPerfect:
        typeof parsed.strictBitPerfect === 'boolean'
          ? parsed.strictBitPerfect
          : fallbackSettings.strictBitPerfect,
      softwareVolume:
        typeof parsed.softwareVolume === 'boolean'
          ? parsed.softwareVolume
          : fallbackSettings.softwareVolume,
      volumePercent: normalizeVolumePercent(parsed.volumePercent),
      theme: isThemeMode(parsed.theme) ? parsed.theme : fallbackSettings.theme,
      fullscreenShowFavorite:
        typeof parsed.fullscreenShowFavorite === 'boolean'
          ? parsed.fullscreenShowFavorite
          : fallbackSettings.fullscreenShowFavorite,
      fullscreenShowPlayMode:
        typeof parsed.fullscreenShowPlayMode === 'boolean'
          ? parsed.fullscreenShowPlayMode
          : fallbackSettings.fullscreenShowPlayMode,
      acrylic: typeof parsed.acrylic === 'boolean' ? parsed.acrylic : fallbackSettings.acrylic,
      accentColor:
        typeof parsed.accentColor === 'string' && parsed.accentColor.trim().length > 0
          ? parsed.accentColor
          : fallbackSettings.accentColor,
      libPaths: normalizeLibraryPaths(parsed.libPaths),
      cacheLimitMb: normalizeCacheLimitMb(parsed.cacheLimitMb),
      songMaxCacheAheadMb: normalizeSongMaxCacheAheadMb(parsed.songMaxCacheAheadMb),
      lyricDebug:
        typeof parsed.lyricDebug === 'boolean' ? parsed.lyricDebug : fallbackSettings.lyricDebug
    }
  } catch (error) {
    console.warn('读取设置失败，使用默认配置。', error)
    return fallbackSettings
  }
}

export function persistSettings(settings: PersistedSettings): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(settings))
  localStorage.setItem(LEGACY_SOUND_QUALITY_KEY, settings.soundQuality)
}

// Audio device normalization
export type RawAudioDeviceInfo = {
  id?: unknown
  name?: unknown
  isDefault?: unknown
  isCurrent?: unknown
  is_default?: unknown
  is_current?: unknown
}

export function normalizeOutputDevice(
  device: RawAudioDeviceInfo | null | undefined
): AudioDeviceInfo | null {
  if (!device || typeof device.id !== 'string' || typeof device.name !== 'string') {
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

export function normalizeOutputDevices(
  devices: RawAudioDeviceInfo[] | null | undefined
): AudioDeviceInfo[] {
  if (!Array.isArray(devices)) {
    return []
  }
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

export function buildUnavailableOutputDeviceName(deviceId: string, deviceName = ''): string {
  const normalizedDeviceName = deviceName.trim()
  return normalizedDeviceName ? `${normalizedDeviceName} (当前不可用)` : `[${deviceId}] 当前不可用`
}

export function withConfiguredOutputDevice(
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

export function findConfiguredOutputDevice(
  devices: AudioDeviceInfo[],
  configuredDeviceId: string
): AudioDeviceInfo | null {
  if (configuredDeviceId === DEFAULT_OUTPUT_DEVICE_ID) {
    return (
      devices.find((device) => device.id === DEFAULT_OUTPUT_DEVICE_ID) ??
      devices.find((device) => device.isDefault) ??
      devices.find((device) => device.isCurrent) ??
      null
    )
  }

  return devices.find((device) => device.id === configuredDeviceId) ?? null
}

export function resolveConfiguredOutputDeviceName(
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

// Cache normalization
export type RawCacheStats = Partial<
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

export function toFiniteNumber(value: unknown, fallback = 0): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

export function normalizeCacheStats(
  stats: RawCacheStats | null | undefined,
  fallbackMaxBytes: number
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

export function bytesToMegabytes(value: number): number {
  return Math.max(MIN_CACHE_LIMIT_MB, Math.round(value / (1024 * 1024)))
}

export function megabytesToBytes(value: number): number {
  return normalizeCacheLimitMb(value) * 1024 * 1024
}

export function isMissingOutputDeviceError(error: unknown): boolean {
  if (error instanceof Error) {
    const message = error.message.toLowerCase()
    return message.includes('device not found') || message.includes('temporarily unavailable')
  }
  if (typeof error === 'string') {
    const message = error.toLowerCase()
    return message.includes('device not found') || message.includes('temporarily unavailable')
  }
  return false
}

/**
 * 目标端点是否已经生效。注意：空列表 / 无 is_current 标记 ⇒ 只有“系统默认输出”
 * 算生效——这是 Linux 枚举契约（列表只含真实硬件端点，跟随默认时无任何标记），
 * 不是通用语义；平台行为变化时需同步修改此处与 ensureConfiguredOutputDevice。
 */
export function isOutputDeviceActive(devices: AudioDeviceInfo[], targetDeviceId: string): boolean {
  if (devices.length === 0) {
    // 设备列表为空（枚举失败/尚未就绪）时只有“跟随系统默认”可以视为生效；
    // 具名设备必须走切换流程：ensureConfiguredOutputDevice 依赖这里的 false
    // 触发 switch→失败→回退默认，从而摘掉独占/BitPerfect——若对具名目标
    // 也返回 true，播放会带着独占选项打到实际生效的虚拟端点上被原生侧拒绝。
    return targetDeviceId === DEFAULT_OUTPUT_DEVICE_ID
  }

  const currentDevice = devices.find((device) => device.isCurrent)

  if (!currentDevice) {
    // Linux 跟随系统默认时，设备列表只枚举真实硬件端点，任何设备都不带
    // is_current 标记——此时“系统默认输出”本身就是生效状态。若不识别这种
    // 情况，每次切歌都会发起一次注定 no-op 的切换 IPC 外加第二次全量
    // ALSA 设备枚举。指定了具名设备时仍视为未生效，走切换流程。
    return targetDeviceId === DEFAULT_OUTPUT_DEVICE_ID
  }

  if (targetDeviceId === DEFAULT_OUTPUT_DEVICE_ID) {
    return currentDevice.isDefault
  }

  return currentDevice.id === targetDeviceId
}
