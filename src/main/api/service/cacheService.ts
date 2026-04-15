import fs from 'fs'
import path from 'path'
import { app } from 'electron'
import {
  getNativeModule,
  type NativeCacheBinding,
  type NativeCacheStats
} from '../native/loadNativeModule'
import { createCacheAssetUrl } from '../protocol/registerCacheProtocol'

export type CacheBucket = 'song' | 'entity' | 'cover' | 'lyric'

export interface CacheStats {
  totalBytes: number
  maxSizeBytes: number
  songBytes: number
  songEntries: number
  entityBytes: number
  entityEntries: number
  coverBytes: number
  coverEntries: number
  lyricBytes: number
  lyricEntries: number
}

export interface CachedSongSource {
  type: 'file' | 'url'
  value: string
  cachePath?: string
  metadataPath?: string
  cacheAheadSecs?: number
}

type CacheableJsonValue = unknown

let nativeCache: NativeCacheBinding | null = null
const resolvedMediaUrlCache = new Map<string, string>()

function getCacheRootDir(): string {
  return path.join(app.getPath('userData'), 'cache')
}

function getNativeCache(): NativeCacheBinding {
  if (!nativeCache) {
    const { CacheService } = getNativeModule()
    nativeCache = new CacheService(getCacheRootDir())
  }

  return nativeCache
}

function toNumber(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0
}

function normalizeCacheStats(raw: NativeCacheStats | null | undefined): CacheStats {
  return {
    totalBytes: toNumber(raw?.totalBytes ?? raw?.total_bytes),
    maxSizeBytes: toNumber(raw?.maxSizeBytes ?? raw?.max_size_bytes),
    songBytes: toNumber(raw?.songBytes ?? raw?.song_bytes),
    songEntries: toNumber(raw?.songEntries ?? raw?.song_entries),
    entityBytes: toNumber(raw?.entityBytes ?? raw?.entity_bytes),
    entityEntries: toNumber(raw?.entityEntries ?? raw?.entity_entries),
    coverBytes: toNumber(raw?.coverBytes ?? raw?.cover_bytes),
    coverEntries: toNumber(raw?.coverEntries ?? raw?.cover_entries),
    lyricBytes: toNumber(raw?.lyricBytes ?? raw?.lyric_bytes),
    lyricEntries: toNumber(raw?.lyricEntries ?? raw?.lyric_entries)
  }
}

function normalizeCachedPath(value: string | null | undefined): string | null {
  if (typeof value !== 'string') {
    return null
  }

  const normalizedPath = value.trim()
  return normalizedPath.length > 0 ? normalizedPath : null
}

function normalizeCachedSongSource(
  raw: Awaited<ReturnType<NativeCacheBinding['prepareSongSource']>> | null | undefined
): CachedSongSource | null {
  if (!raw || typeof raw !== 'object') {
    return null
  }

  const type = raw.type === 'file' || raw.type === 'url' ? raw.type : null
  const value = typeof raw.value === 'string' ? raw.value.trim() : ''
  if (!type) {
    return null
  }

  const cachePath = normalizeCachedPath(raw.cachePath ?? raw.cache_path)
  const metadataPath = normalizeCachedPath(raw.metadataPath ?? raw.metadata_path)
  const cacheAheadSecs =
    typeof (raw.cacheAheadSecs ?? raw.cache_ahead_secs) === 'number' &&
    Number.isFinite(raw.cacheAheadSecs ?? raw.cache_ahead_secs)
      ? Number(raw.cacheAheadSecs ?? raw.cache_ahead_secs)
      : undefined

  return {
    type,
    value,
    ...(cachePath ? { cachePath } : {}),
    ...(metadataPath ? { metadataPath } : {}),
    ...(cacheAheadSecs !== undefined ? { cacheAheadSecs } : {})
  }
}

function isExistingCachedPath(filePath: string): boolean {
  return fs.existsSync(filePath)
}

function stableStringify(value: unknown): string {
  if (Array.isArray(value)) {
    return `[${value.map((item) => stableStringify(item)).join(',')}]`
  }

  if (value && typeof value === 'object') {
    return `{${Object.entries(value as Record<string, unknown>)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, nestedValue]) => `${JSON.stringify(key)}:${stableStringify(nestedValue)}`)
      .join(',')}}`
  }

  return JSON.stringify(value) ?? 'null'
}

export function buildCacheKey(value: unknown): string {
  return stableStringify(value)
}

async function readJson<T>(bucket: CacheBucket, key: string): Promise<T | null> {
  const raw = await getNativeCache().getJson(bucket, key)
  if (typeof raw !== 'string' || raw.trim().length === 0) {
    return null
  }

  try {
    return JSON.parse(raw) as T
  } catch (error) {
    console.warn(`[cache] failed to parse cached ${bucket} payload`, error)
    return null
  }
}

async function writeJson(
  bucket: CacheBucket,
  key: string,
  value: CacheableJsonValue
): Promise<CacheStats> {
  return normalizeCacheStats(await getNativeCache().putJson(bucket, key, JSON.stringify(value)))
}

export const CacheService = {
  buildKey: buildCacheKey,

  async getStats(): Promise<CacheStats> {
    return normalizeCacheStats(await getNativeCache().getStats())
  },

  async setMaxSizeBytes(maxSizeBytes: number): Promise<CacheStats> {
    return normalizeCacheStats(await getNativeCache().setMaxSizeBytes(Math.max(0, maxSizeBytes)))
  },

  async getSongCacheAheadSecs(): Promise<number> {
    return Math.max(5, Number(await getNativeCache().getSongCacheAheadSecs()) || 30)
  },

  async setSongCacheAheadSecs(songCacheAheadSecs: number): Promise<number> {
    return Math.max(
      5,
      Number(await getNativeCache().setSongCacheAheadSecs(Math.max(5, songCacheAheadSecs))) || 30
    )
  },

  async clear(): Promise<CacheStats> {
    resolvedMediaUrlCache.clear()
    return normalizeCacheStats(await getNativeCache().clear())
  },

  async getJson<T>(bucket: CacheBucket, key: string): Promise<T | null> {
    return readJson<T>(bucket, key)
  },

  async setJson(bucket: CacheBucket, key: string, value: CacheableJsonValue): Promise<CacheStats> {
    return writeJson(bucket, key, value)
  },

  async getOrSetJson<T extends CacheableJsonValue>(
    bucket: CacheBucket,
    key: string,
    producer: () => Promise<T | null | undefined>
  ): Promise<T | null> {
    const cached = await readJson<T>(bucket, key)
    if (cached !== null) {
      return cached
    }

    const freshValue = await producer()
    if (freshValue === null || freshValue === undefined) {
      return null
    }

    await writeJson(bucket, key, freshValue)
    return freshValue
  },

  async resolveCachedMediaUrl(url: string): Promise<string> {
    const normalizedUrl = url.trim()
    if (!normalizedUrl) {
      return ''
    }

    if (
      normalizedUrl.startsWith('ncm-cache:') ||
      normalizedUrl.startsWith('file:') ||
      normalizedUrl.startsWith('data:') ||
      normalizedUrl.startsWith('blob:')
    ) {
      return normalizedUrl
    }

    const cachedUrl = resolvedMediaUrlCache.get(normalizedUrl)
    if (cachedUrl && isExistingCachedPath(cachedUrl)) {
      return createCacheAssetUrl(cachedUrl)
    }
    resolvedMediaUrlCache.delete(normalizedUrl)

    try {
      const cachedPath = normalizeCachedPath(
        await getNativeCache().cacheRemoteFile('cover', normalizedUrl, normalizedUrl)
      )
      if (!cachedPath) {
        return normalizedUrl
      }

      resolvedMediaUrlCache.set(normalizedUrl, cachedPath)
      return createCacheAssetUrl(cachedPath)
    } catch (error) {
      console.warn('[cache] failed to resolve cached media url', error)
      return normalizedUrl
    }
  },

  async prepareSongSource(payload: {
    songId: number
    quality: string
    url: string
  }): Promise<CachedSongSource> {
    const { songId, quality, url } = payload
    const normalizedUrl = url.trim()

    if (!normalizedUrl) {
      return { type: 'url', value: '' }
    }

    try {
      const source = normalizeCachedSongSource(
        await getNativeCache().prepareSongSource(songId, quality, normalizedUrl)
      )
      if (source) {
        return source
      }
    } catch (error) {
      console.warn('[cache] failed to prepare cached song source', error)
    }

    return {
      type: 'url',
      value: normalizedUrl
    }
  }
}
