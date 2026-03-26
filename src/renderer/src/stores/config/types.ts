import { SoundQualityType } from '@renderer/types/song'

export type AudioEngineType = 'native' | 'webapi' | 'auto'
export type ThemeMode = 'light' | 'dark' | 'adaptive'

export interface PersistedSettings {
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

export const STORAGE_KEY = 'app_settings'
export const LEGACY_SOUND_QUALITY_KEY = 'sound_quality'
export const DEFAULT_OUTPUT_DEVICE_ID = 'default'
export const DEFAULT_OUTPUT_DEVICE_NAME = '系统默认输出'
export const DEFAULT_CACHE_LIMIT_MB = 512
export const MIN_CACHE_LIMIT_MB = 128
export const MAX_CACHE_LIMIT_MB = 8192
export const DEFAULT_SONG_CACHE_AHEAD_SECS = 30
export const MIN_SONG_CACHE_AHEAD_SECS = 10
export const MAX_SONG_CACHE_AHEAD_SECS = 300

export const SOUND_QUALITIES: SoundQualityType[] = [
  'standard',
  'exhigh',
  'lossless',
  'hires',
  'jyeffect',
  'sky',
  'jymaster'
]
export const AUDIO_ENGINES: AudioEngineType[] = ['native', 'webapi', 'auto']
export const THEMES: ThemeMode[] = ['light', 'dark', 'adaptive']

export const DEFAULT_SETTINGS: PersistedSettings = {
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

export type { SoundQualityType }
