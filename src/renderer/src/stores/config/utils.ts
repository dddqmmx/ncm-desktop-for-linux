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
  MAX_SONG_CACHE_AHEAD_SECS,
  MIN_SONG_CACHE_AHEAD_SECS
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

export function normalizeSongCacheAheadSecs(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return DEFAULT_SETTINGS.songCacheAheadSecs
  }

  return Math.min(MAX_SONG_CACHE_AHEAD_SECS, Math.max(MIN_SONG_CACHE_AHEAD_SECS, Math.round(value)))
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

export function normalizeOutputDevice(device: RawAudioDeviceInfo | null | undefined): AudioDeviceInfo | null {
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

export function normalizeOutputDevices(devices: RawAudioDeviceInfo[] | null | undefined): AudioDeviceInfo[] {
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

export function isOutputDeviceActive(devices: AudioDeviceInfo[], targetDeviceId: string): boolean {
  if (devices.length === 0) {
    // If we have no device list yet, assume the status quo is fine
    // to avoid redundant switch attempts during early initialization.
    return true
  }

  const currentDevice = devices.find((device) => device.isCurrent)

  if (!currentDevice) {
    return false
  }

  if (targetDeviceId === DEFAULT_OUTPUT_DEVICE_ID) {
    return currentDevice.isDefault
  }

  return currentDevice.id === targetDeviceId
}
