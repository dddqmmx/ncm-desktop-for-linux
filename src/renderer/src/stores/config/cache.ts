import { defineStore } from 'pinia'
import { ref, type Ref } from 'vue'
import { CacheStats } from '@renderer/types/cache'
import { clearResolvedMediaUrlCache } from '@renderer/utils/cache'
import {
  loadSettings,
  normalizeCacheStats,
  megabytesToBytes,
  bytesToMegabytes,
  normalizeCacheLimitMb,
  normalizeSongCacheAheadSecs,
  normalizeSongMaxCacheAheadMb,
  RawCacheStats
} from './utils'

export const useCacheConfigStore = defineStore('cacheConfig', () => {
  const initialSettings = loadSettings()

  const libPaths = ref<string[]>(initialSettings.libPaths)
  const cacheLimitMb = ref(initialSettings.cacheLimitMb)
  const songCacheAheadSecs = ref(initialSettings.songCacheAheadSecs)
  const songMaxCacheAheadMb = ref(initialSettings.songMaxCacheAheadMb)

  const cacheStats = ref<CacheStats>(
    normalizeCacheStats(null, megabytesToBytes(cacheLimitMb.value))
  )
  const isLoadingCacheStats = ref(false)
  const isUpdatingCacheLimit = ref(false)
  const isUpdatingSongCacheAheadSecs = ref(false)
  const isUpdatingSongMaxCacheAheadBytes = ref(false)
  const isClearingCache = ref(false)
  const cacheError = ref('')

  async function withUpdateGuard(
    flag: Ref<boolean>,
    errorPrefix: string,
    action: () => Promise<void>
  ): Promise<boolean> {
    flag.value = true
    cacheError.value = ''
    try {
      await action()
      return true
    } catch (error) {
      cacheError.value = `${errorPrefix}失败，请重试。`
      console.error(`${errorPrefix}失败`, error)
      await refreshCacheStats()
      return false
    } finally {
      flag.value = false
    }
  }

  const refreshCacheStats = async (): Promise<CacheStats> => {
    isLoadingCacheStats.value = true
    cacheError.value = ''

    try {
      const [rawStats, rawSongCacheAheadSecs, rawSongMaxCacheAheadBytes] = await Promise.all([
        window.api.cache_get_stats(),
        window.api.cache_get_song_cache_ahead_secs(),
        window.api.cache_get_song_max_cache_ahead_bytes()
      ])
      const stats = normalizeCacheStats(
        rawStats as RawCacheStats,
        megabytesToBytes(cacheLimitMb.value)
      )
      cacheStats.value = stats
      cacheLimitMb.value = normalizeCacheLimitMb(bytesToMegabytes(stats.maxSizeBytes))
      songCacheAheadSecs.value = normalizeSongCacheAheadSecs(rawSongCacheAheadSecs)
      songMaxCacheAheadMb.value = normalizeSongMaxCacheAheadMb(
        Number(rawSongMaxCacheAheadBytes) / (1024 * 1024)
      )
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
    const normalizedLimitMb = normalizeCacheLimitMb(nextLimitMb)
    return withUpdateGuard(isUpdatingCacheLimit, '更新缓存上限', async () => {
      const stats = normalizeCacheStats(
        (await window.api.cache_set_max_size(megabytesToBytes(normalizedLimitMb))) as RawCacheStats,
        megabytesToBytes(normalizedLimitMb)
      )
      cacheStats.value = stats
      cacheLimitMb.value = normalizeCacheLimitMb(bytesToMegabytes(stats.maxSizeBytes))
    })
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
    const normalizedSecs = normalizeSongCacheAheadSecs(nextSecs)
    return withUpdateGuard(isUpdatingSongCacheAheadSecs, '更新歌曲预缓存时长', async () => {
      songCacheAheadSecs.value = normalizeSongCacheAheadSecs(
        await window.api.cache_set_song_cache_ahead_secs(normalizedSecs)
      )
    })
  }

  const setSongMaxCacheAheadSize = async (nextMb: number): Promise<boolean> => {
    const normalizedMb = normalizeSongMaxCacheAheadMb(nextMb)
    return withUpdateGuard(isUpdatingSongMaxCacheAheadBytes, '更新歌曲最大预下载大小', async () => {
      songMaxCacheAheadMb.value = normalizeSongMaxCacheAheadMb(
        Number(await window.api.cache_set_song_max_cache_ahead_bytes(normalizedMb * 1024 * 1024)) /
          (1024 * 1024)
      )
    })
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
    libPaths,
    cacheLimitMb,
    songCacheAheadSecs,
    songMaxCacheAheadMb,
    cacheStats,
    isLoadingCacheStats,
    isUpdatingCacheLimit,
    isUpdatingSongCacheAheadSecs,
    isUpdatingSongMaxCacheAheadBytes,
    isClearingCache,
    cacheError,
    refreshCacheStats,
    setCacheLimit,
    clearCache,
    setSongCacheAheadTime,
    setSongMaxCacheAheadSize,
    addLibraryPath,
    removeLibraryPath
  }
})
