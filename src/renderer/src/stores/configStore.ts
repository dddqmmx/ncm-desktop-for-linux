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
    softwareVolume: audio.softwareVolume,
    volumePercent: audio.volumePercent,
    theme: appearance.theme,
    fullscreenShowFavorite: appearance.fullscreenShowFavorite,
    fullscreenShowPlayMode: appearance.fullscreenShowPlayMode,
    acrylic: appearance.acrylic,
    accentColor: appearance.accentColor,
    libPaths: [...cache.libPaths],
    cacheLimitMb: cache.cacheLimitMb,
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
    audio.setAudioEngine(settings.audioEngine)
    audio.outputDeviceId = settings.outputDeviceId
    audio.outputDeviceName = settings.outputDeviceName
    audio.setExclusiveMode(settings.exclusiveMode)
    audio.setStrictBitPerfect(settings.strictBitPerfect)
    audio.setSoftwareVolume(settings.softwareVolume)
    audio.setVolumePercent(settings.volumePercent)
    appearance.theme = settings.theme
    appearance.fullscreenShowFavorite = settings.fullscreenShowFavorite
    appearance.fullscreenShowPlayMode = settings.fullscreenShowPlayMode
    appearance.acrylic = settings.acrylic
    appearance.accentColor = settings.accentColor
    cache.libPaths = [...settings.libPaths]
    cache.cacheLimitMb = settings.cacheLimitMb
    cache.songMaxCacheAheadMb = settings.songMaxCacheAheadMb
    debug.lyricDebug = settings.lyricDebug
  }

  const settingsChannel =
    typeof window !== 'undefined' && 'BroadcastChannel' in window
      ? new BroadcastChannel('app-settings')
      : null

  // 设置持久化 + 跨窗口广播的尾沿去抖。音量滑块拖拽时每个 1% 步进都会触发本
  // watch；持久化是全量 snapshot 分配 + JSON.stringify + 两次同步
  // localStorage 写 + BroadcastChannel 广播，逐帧执行纯属浪费（IPC 侧的
  // set_volume 早已做了同样的 80ms 尾沿节流）。期间后续触发只重置计时器，
  // 停顿 120ms 后落最后一帧。
  const SETTINGS_PERSIST_DEBOUNCE_MS = 120
  let persistDebounceTimer: ReturnType<typeof setTimeout> | null = null
  let isPersistPending = false

  const flushSettingsPersist = (): void => {
    if (!isPersistPending) return
    isPersistPending = false

    const nextSettings = snapshotSettings()
    persistSettings(nextSettings)
    settingsChannel?.postMessage({
      type: 'settings-updated',
      settings: nextSettings
    })
  }

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
      () => audio.softwareVolume,
      () => audio.volumePercent,
      () => appearance.theme,
      () => appearance.fullscreenShowFavorite,
      () => appearance.fullscreenShowPlayMode,
      () => appearance.acrylic,
      () => appearance.accentColor,
      () => cache.libPaths,
      () => cache.cacheLimitMb,
      () => cache.songMaxCacheAheadMb,
      () => debug.lyricDebug
    ],
    () => {
      // 外部（另一窗口）应用设置触发的本回调不持久化、不回声广播。吞在触发
      // 时完成而不是去抖 flush 时：否则外部应用后 120ms 内的本地修改会被
      // 整个吞掉。若此时另有待 flush 的本地修改，合并后的快照与对端等价
      // （settingsEqual 幂等），回声无害。
      if (skipNextSettingsBroadcast) {
        skipNextSettingsBroadcast = false
        return
      }

      isPersistPending = true
      if (persistDebounceTimer !== null) {
        clearTimeout(persistDebounceTimer)
      }
      persistDebounceTimer = setTimeout(() => {
        persistDebounceTimer = null
        flushSettingsPersist()
      }, SETTINGS_PERSIST_DEBOUNCE_MS)
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

  // 去抖窗口内关窗（设置窗口尤其常见）会把最后一帧设置一起带走，卸载前冲刷。
  if (typeof window !== 'undefined' && typeof window.addEventListener === 'function') {
    window.addEventListener('beforeunload', flushSettingsPersist)
  }

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
      set: (val) => audio.setAudioEngine(val)
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
      set: (val) => audio.setExclusiveMode(val)
    }),
    strictBitPerfect: computed({
      get: () => audio.strictBitPerfect,
      set: (val) => audio.setStrictBitPerfect(val)
    }),
    softwareVolume: computed({
      get: () => audio.softwareVolume,
      set: (val) => audio.setSoftwareVolume(val)
    }),
    volumePercent: computed({
      get: () => audio.volumePercent,
      set: (val) => audio.setVolumePercent(val)
    }),
    outputDevices: computed(() => audio.outputDevices),
    currentOutputDevice: computed(() => audio.currentOutputDevice),
    isLoadingOutputDevices: computed(() => audio.isLoadingOutputDevices),
    isSwitchingOutputDevice: computed(() => audio.isSwitchingOutputDevice),
    outputDeviceError: computed(() => audio.outputDeviceError),
    supportsExclusiveOutput: computed(() => audio.supportsExclusiveOutput),
    canAdjustVolume: computed(() => audio.canAdjustVolume),

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
    fullscreenShowFavorite: computed({
      get: () => appearance.fullscreenShowFavorite,
      set: (val) => (appearance.fullscreenShowFavorite = val)
    }),
    fullscreenShowPlayMode: computed({
      get: () => appearance.fullscreenShowPlayMode,
      set: (val) => (appearance.fullscreenShowPlayMode = val)
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
    songMaxCacheAheadMb: computed({
      get: () => cache.songMaxCacheAheadMb,
      set: (val) => (cache.songMaxCacheAheadMb = val)
    }),
    songMaxCacheAheadBytes: computed(() => cache.songMaxCacheAheadMb * 1024 * 1024),
    cacheStats: computed(() => cache.cacheStats),
    isLoadingCacheStats: computed(() => cache.isLoadingCacheStats),
    isUpdatingCacheLimit: computed(() => cache.isUpdatingCacheLimit),
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
    outputOptionsFor: audio.outputOptionsFor,
    setSoundQuality: (quality: SoundQualityType) => (audio.soundQuality = quality),
    setOutputDevice: (deviceId: string) => audio.applyOutputDevice(deviceId, true),
    setCacheLimit: cache.setCacheLimit,
    setSongMaxCacheAheadSize: cache.setSongMaxCacheAheadSize,
    clearCache: cache.clearCache,
    addLibraryPath: cache.addLibraryPath,
    removeLibraryPath: cache.removeLibraryPath
  }
})
