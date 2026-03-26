import { defineStore } from 'pinia'
import { watch, computed } from 'vue'
import { useAudioConfigStore } from './config/audio'
import { useAppearanceConfigStore } from './config/appearance'
import { useCacheConfigStore } from './config/cache'
import { useGeneralConfigStore } from './config/general'
import { persistSettings } from './config/utils'
import { PersistedSettings, SoundQualityType } from './config/types'

export * from './config/types'

export const useConfigStore = defineStore('config', () => {
  const audio = useAudioConfigStore()
  const appearance = useAppearanceConfigStore()
  const cache = useCacheConfigStore()
  const general = useGeneralConfigStore()

  const snapshotSettings = (): PersistedSettings => ({
    soundQuality: audio.soundQuality,
    autoLaunch: general.autoLaunch,
    trayMinimize: general.trayMinimize,
    audioEngine: audio.audioEngine,
    outputDeviceId: audio.outputDeviceId,
    outputDeviceName: audio.outputDeviceName,
    exclusiveMode: audio.exclusiveMode,
    theme: appearance.theme,
    acrylic: appearance.acrylic,
    accentColor: appearance.accentColor,
    libPaths: [...cache.libPaths],
    cacheLimitMb: cache.cacheLimitMb,
    songCacheAheadSecs: cache.songCacheAheadSecs
  })

  watch(
    [
      () => audio.soundQuality,
      () => general.autoLaunch,
      () => general.trayMinimize,
      () => audio.audioEngine,
      () => audio.outputDeviceId,
      () => audio.outputDeviceName,
      () => audio.exclusiveMode,
      () => appearance.theme,
      () => appearance.acrylic,
      () => appearance.accentColor,
      () => cache.libPaths,
      () => cache.cacheLimitMb,
      () => cache.songCacheAheadSecs
    ],
    () => {
      persistSettings(snapshotSettings())
    },
    { deep: true }
  )

  let initializePromise: Promise<void> | null = null

  const initialize = async (): Promise<void> => {
    if (initializePromise) {
      return initializePromise
    }

    initializePromise = (async () => {
      await Promise.all([audio.ensureConfiguredOutputDevice(), cache.refreshCacheStats()])
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
    cacheStats: computed(() => cache.cacheStats),
    isLoadingCacheStats: computed(() => cache.isLoadingCacheStats),
    isUpdatingCacheLimit: computed(() => cache.isUpdatingCacheLimit),
    isUpdatingSongCacheAheadSecs: computed(() => cache.isUpdatingSongCacheAheadSecs),
    isClearingCache: computed(() => cache.isClearingCache),
    cacheError: computed(() => cache.cacheError),

    // Actions
    initialize,
    refreshOutputDevices: audio.refreshOutputDevices,
    refreshCacheStats: cache.refreshCacheStats,
    ensureConfiguredOutputDevice: audio.ensureConfiguredOutputDevice,
    setSoundQuality: (quality: SoundQualityType) => (audio.soundQuality = quality),
    setOutputDevice: (deviceId: string) => audio.applyOutputDevice(deviceId, true),
    setCacheLimit: cache.setCacheLimit,
    setSongCacheAheadTime: cache.setSongCacheAheadTime,
    clearCache: cache.clearCache,
    addLibraryPath: cache.addLibraryPath,
    removeLibraryPath: cache.removeLibraryPath
  }
})
