import { defineStore } from 'pinia'
import { ref } from 'vue'
import { CacheStats } from '@renderer/types/cache'
import { clearResolvedMediaUrlCache } from '@renderer/utils/cache'
import {
  loadSettings,
  normalizeCacheStats,
  megabytesToBytes,
  bytesToMegabytes,
  normalizeCacheLimitMb,
  normalizeSongCacheAheadSecs,
  RawCacheStats
} from './utils'

export const useCacheConfigStore = defineStore('cacheConfig', () => {
  const initialSettings = loadSettings()

  const libPaths = ref<string[]>(initialSettings.libPaths)
  const cacheLimitMb = ref(initialSettings.cacheLimitMb)
  const songCacheAheadSecs = ref(initialSettings.songCacheAheadSecs)

  const cacheStats = ref<CacheStats>(
    normalizeCacheStats(null, megabytesToBytes(cacheLimitMb.value))
  )
  const isLoadingCacheStats = ref(false)
  const isUpdatingCacheLimit = ref(false)
  const isUpdatingSongCacheAheadSecs = ref(false)
  const isClearingCache = ref(false)
  const cacheError = ref('')

  const refreshCacheStats = async (): Promise<CacheStats> => {
    isLoadingCacheStats.value = true
    cacheError.value = ''

    try {
      const [rawStats, rawSongCacheAheadSecs] = await Promise.all([
        window.api.cache_get_stats(),
        window.api.cache_get_song_cache_ahead_secs()
      ])
      const stats = normalizeCacheStats(
        rawStats as RawCacheStats,
        megabytesToBytes(cacheLimitMb.value)
      )
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
    cacheStats,
    isLoadingCacheStats,
    isUpdatingCacheLimit,
    isUpdatingSongCacheAheadSecs,
    isClearingCache,
    cacheError,
    refreshCacheStats,
    setCacheLimit,
    clearCache,
    setSongCacheAheadTime,
    addLibraryPath,
    removeLibraryPath
  }
})
