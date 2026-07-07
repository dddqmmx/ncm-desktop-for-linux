import { defineStore } from 'pinia'
import { watch, computed } from 'vue'
import { useAudioConfigStore } from './config/audio'
import { useAppearanceConfigStore } from './config/appearance'
import { useCacheConfigStore } from './config/cache'
import { useDebugConfigStore } from './config/debug'
import { useGeneralConfigStore } from './config/general'
import { loadSettings, persistSettings } from './config/utils'
import { PersistedSettings, SoundQualityType, STORAGE_KEY } from './config/types'
import { applySystemTheme } from '@renderer/utils/theme'

export * from './config/types'

export const useConfigStore = defineStore('config', () => {
  const audio = useAudioConfigStore()
  const appearance = useAppearanceConfigStore()
  const cache = useCacheConfigStore()
  const debug = useDebugConfigStore()
  const general = useGeneralConfigStore()

  const snapshotSettings = (): PersistedSettings => ({
    soundQuality: audio.soundQuality,
    autoLaunch: general.autoLaunch,
    trayMinimize: general.trayMinimize,
    audioEngine: audio.audioEngine,
    outputDeviceId: audio.outputDeviceId,
    outputDeviceName: audio.outputDeviceName,
    exclusiveMode: audio.exclusiveMode,
    strictBitPerfect: audio.strictBitPerfect,
    theme: appearance.theme,
    acrylic: appearance.acrylic,
    accentColor: appearance.accentColor,
    libPaths: [...cache.libPaths],
    cacheLimitMb: cache.cacheLimitMb,
    songCacheAheadSecs: cache.songCacheAheadSecs,
    songMaxCacheAheadMb: cache.songMaxCacheAheadMb,
    lyricDebug: debug.lyricDebug
  })

  let skipNextSettingsBroadcast = false

  const settingsEqual = (left: PersistedSettings, right: PersistedSettings): boolean =>
    JSON.stringify(left) === JSON.stringify(right)

  const applyExternalSettings = (settings: PersistedSettings): void => {
    if (settingsEqual(snapshotSettings(), settings)) {
      return
    }

    skipNextSettingsBroadcast = true
    audio.soundQuality = settings.soundQuality
    general.autoLaunch = settings.autoLaunch
    general.trayMinimize = settings.trayMinimize
    audio.audioEngine = settings.audioEngine
    audio.outputDeviceId = settings.outputDeviceId
    audio.outputDeviceName = settings.outputDeviceName
    audio.exclusiveMode = settings.exclusiveMode
    audio.strictBitPerfect = settings.strictBitPerfect
    appearance.theme = settings.theme
    appearance.acrylic = settings.acrylic
    appearance.accentColor = settings.accentColor
    cache.libPaths = [...settings.libPaths]
    cache.cacheLimitMb = settings.cacheLimitMb
    cache.songCacheAheadSecs = settings.songCacheAheadSecs
    cache.songMaxCacheAheadMb = settings.songMaxCacheAheadMb
    debug.lyricDebug = settings.lyricDebug
  }

  const settingsChannel =
    typeof window !== 'undefined' && 'BroadcastChannel' in window
      ? new BroadcastChannel('app-settings')
      : null

  watch(
    [
      () => audio.soundQuality,
      () => general.autoLaunch,
      () => general.trayMinimize,
      () => audio.audioEngine,
      () => audio.outputDeviceId,
      () => audio.outputDeviceName,
      () => audio.exclusiveMode,
      () => audio.strictBitPerfect,
      () => appearance.theme,
      () => appearance.acrylic,
      () => appearance.accentColor,
      () => cache.libPaths,
      () => cache.cacheLimitMb,
      () => cache.songCacheAheadSecs,
      () => cache.songMaxCacheAheadMb,
      () => debug.lyricDebug
    ],
    () => {
      if (skipNextSettingsBroadcast) {
        skipNextSettingsBroadcast = false
        return
      }

      const nextSettings = snapshotSettings()
      persistSettings(nextSettings)
      settingsChannel?.postMessage({
        type: 'settings-updated',
        settings: nextSettings
      })
    },
    { deep: true }
  )

  watch(
    [() => appearance.theme, () => appearance.accentColor],
    ([theme, accentColor]) => {
      applySystemTheme(theme, accentColor)
    },
    { immediate: true }
  )

  if (typeof window !== 'undefined' && typeof window.matchMedia === 'function') {
    const systemColorScheme = window.matchMedia('(prefers-color-scheme: dark)')
    systemColorScheme.addEventListener('change', () => {
      if (appearance.theme === 'adaptive') {
        applySystemTheme(appearance.theme, appearance.accentColor)
      }
    })
  }

  settingsChannel?.addEventListener('message', (event) => {
    if (event.data?.type !== 'settings-updated') {
      return
    }

    applyExternalSettings(loadSettings())
  })

  if (typeof window !== 'undefined' && typeof window.addEventListener === 'function') {
    window.addEventListener('storage', (event) => {
      if (event.key === STORAGE_KEY) {
        applyExternalSettings(loadSettings())
      }
    })
  }

  let initializePromise: Promise<void> | null = null

  const initialize = async (): Promise<void> => {
    if (initializePromise) {
      return initializePromise
    }

    initializePromise = (async () => {
      await Promise.all([audio.refreshOutputDevices(), cache.refreshCacheStats()])
    })().catch((error) => {
      initializePromise = null
      console.error('初始化设置失败', error)
      throw error
    })

    return initializePromise
  }

  return {
    // Audio
    soundQuality: computed({
      get: () => audio.soundQuality,
      set: (val) => (audio.soundQuality = val)
    }),
    audioEngine: computed({
      get: () => audio.audioEngine,
      set: (val) => (audio.audioEngine = val)
    }),
    outputDeviceId: computed({
      get: () => audio.outputDeviceId,
      set: (val) => (audio.outputDeviceId = val)
    }),
    outputDeviceName: computed({
      get: () => audio.outputDeviceName,
      set: (val) => (audio.outputDeviceName = val)
    }),
    exclusiveMode: computed({
      get: () => audio.exclusiveMode,
      set: (val) => (audio.exclusiveMode = val)
    }),
    strictBitPerfect: computed({
      get: () => audio.strictBitPerfect,
      set: (val) => (audio.strictBitPerfect = val)
    }),
    outputDevices: computed(() => audio.outputDevices),
    currentOutputDevice: computed(() => audio.currentOutputDevice),
    isLoadingOutputDevices: computed(() => audio.isLoadingOutputDevices),
    isSwitchingOutputDevice: computed(() => audio.isSwitchingOutputDevice),
    outputDeviceError: computed(() => audio.outputDeviceError),

    // Appearance
    theme: computed({
      get: () => appearance.theme,
      set: (val) => (appearance.theme = val)
    }),
    acrylic: computed({
      get: () => appearance.acrylic,
      set: (val) => (appearance.acrylic = val)
    }),
    accentColor: computed({
      get: () => appearance.accentColor,
      set: (val) => (appearance.accentColor = val)
    }),

    // General
    autoLaunch: computed({
      get: () => general.autoLaunch,
      set: (val) => (general.autoLaunch = val)
    }),
    trayMinimize: computed({
      get: () => general.trayMinimize,
      set: (val) => (general.trayMinimize = val)
    }),

    // Cache
    libPaths: computed({
      get: () => cache.libPaths,
      set: (val) => (cache.libPaths = val)
    }),
    cacheLimitMb: computed({
      get: () => cache.cacheLimitMb,
      set: (val) => (cache.cacheLimitMb = val)
    }),
    songCacheAheadSecs: computed({
      get: () => cache.songCacheAheadSecs,
      set: (val) => (cache.songCacheAheadSecs = val)
    }),
    songMaxCacheAheadMb: computed({
      get: () => cache.songMaxCacheAheadMb,
      set: (val) => (cache.songMaxCacheAheadMb = val)
    }),
    songMaxCacheAheadBytes: computed(() => cache.songMaxCacheAheadMb * 1024 * 1024),
    cacheStats: computed(() => cache.cacheStats),
    isLoadingCacheStats: computed(() => cache.isLoadingCacheStats),
    isUpdatingCacheLimit: computed(() => cache.isUpdatingCacheLimit),
    isUpdatingSongCacheAheadSecs: computed(() => cache.isUpdatingSongCacheAheadSecs),
    isUpdatingSongMaxCacheAheadBytes: computed(() => cache.isUpdatingSongMaxCacheAheadBytes),
    isClearingCache: computed(() => cache.isClearingCache),
    cacheError: computed(() => cache.cacheError),

    // Debug
    lyricDebug: computed({
      get: () => debug.lyricDebug,
      set: (val) => (debug.lyricDebug = val)
    }),

    // Actions
    initialize,
    refreshOutputDevices: audio.refreshOutputDevices,
    refreshCacheStats: cache.refreshCacheStats,
    ensureConfiguredOutputDevice: audio.ensureConfiguredOutputDevice,
    setSoundQuality: (quality: SoundQualityType) => (audio.soundQuality = quality),
    setOutputDevice: (deviceId: string) => audio.applyOutputDevice(deviceId, true),
    setCacheLimit: cache.setCacheLimit,
    setSongCacheAheadTime: cache.setSongCacheAheadTime,
    setSongMaxCacheAheadSize: cache.setSongMaxCacheAheadSize,
    clearCache: cache.clearCache,
    addLibraryPath: cache.addLibraryPath,
    removeLibraryPath: cache.removeLibraryPath
  }
})
