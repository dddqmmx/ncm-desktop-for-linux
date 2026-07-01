import type {
  CachedSongSource,
  PlaybackCacheRequest,
  PreparedPlaybackCache
} from '@renderer/types/cache'

function normalizeUrl(value: string | null | undefined): string {
  return typeof value === 'string' ? value.trim() : ''
}

const LOCAL_SCHEMES = ['ncm-cache:', 'file:', 'data:', 'blob:']

export function isLocalScheme(url: string): boolean {
  return LOCAL_SCHEMES.some((scheme) => url.startsWith(scheme))
}

export function clearResolvedMediaUrlCache(): void {
  // renderer 不再保留长期路径映射，避免淘汰后出现悬空 file URL
}

export async function resolveCachedMediaUrl(url: string | null | undefined): Promise<string> {
  const normalizedUrl = normalizeUrl(url)
  if (!normalizedUrl) {
    return ''
  }

  if (isLocalScheme(normalizedUrl)) {
    return normalizedUrl
  }

  try {
    const resolvedUrl = await window.api.resolve_cached_media_url(normalizedUrl)
    return normalizeUrl(resolvedUrl) || normalizedUrl
  } catch (error) {
    console.warn('解析缓存媒体地址失败', error)
    return normalizedUrl
  }
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value)
}

function normalizeSongSourcePayload(payload: unknown): CachedSongSource | null {
  if (!payload || typeof payload !== 'object') {
    return null
  }

  const raw = payload as Record<string, unknown>
  const type = raw.type === 'file' || raw.type === 'url' ? raw.type : null
  const value = typeof raw.value === 'string' ? raw.value.trim() : ''
  if (!type) {
    return null
  }

  const cachePath =
    typeof raw.cachePath === 'string' && raw.cachePath.trim() ? raw.cachePath.trim() : undefined
  const metadataPath =
    typeof raw.metadataPath === 'string' && raw.metadataPath.trim()
      ? raw.metadataPath.trim()
      : undefined
  const cacheAheadSecs = isFiniteNumber(raw.cacheAheadSecs) ? raw.cacheAheadSecs : undefined
  const maxCacheAheadBytes = isFiniteNumber(raw.maxCacheAheadBytes)
    ? raw.maxCacheAheadBytes
    : undefined

  return {
    type,
    value,
    ...(cachePath ? { cachePath } : {}),
    ...(metadataPath ? { metadataPath } : {}),
    ...(cacheAheadSecs !== undefined ? { cacheAheadSecs } : {}),
    ...(maxCacheAheadBytes !== undefined ? { maxCacheAheadBytes } : {})
  }
}

function fallbackUrlSource(url: string): CachedSongSource {
  return { type: 'url', value: url }
}

export async function cacheSongSource(
  songId: number,
  quality: string,
  url: string,
  expectedBytes?: number,
  durationMs?: number
): Promise<CachedSongSource> {
  const normalizedUrl = normalizeUrl(url)
  if (!normalizedUrl) {
    return { type: 'url', value: '' }
  }

  try {
    const payload = await window.api.cache_song_source({
      songId,
      quality,
      url: normalizedUrl,
      expectedBytes,
      durationMs
    })
    return normalizeSongSourcePayload(payload) ?? fallbackUrlSource(normalizedUrl)
  } catch (error) {
    console.warn('完整缓存歌曲失败', error)
    return fallbackUrlSource(normalizedUrl)
  }
}

export async function prepareCachedSongSource(
  songId: number,
  quality: string,
  url: string,
  expectedBytes?: number
): Promise<CachedSongSource> {
  const normalizedUrl = normalizeUrl(url)
  if (!normalizedUrl) {
    return { type: 'url', value: '' }
  }

  try {
    const payload = await window.api.prepare_cached_song_source({
      songId,
      quality,
      url: normalizedUrl,
      expectedBytes
    })
    return normalizeSongSourcePayload(payload) ?? fallbackUrlSource(normalizedUrl)
  } catch (error) {
    console.warn('准备缓存歌曲源失败', error)
    return fallbackUrlSource(normalizedUrl)
  }
}

export async function preparePlaybackCache(
  request: PlaybackCacheRequest
): Promise<PreparedPlaybackCache> {
  const source = await prepareCachedSongSource(
    request.songId,
    request.quality,
    request.url,
    request.expectedBytes
  )
  const backgroundCacheRequest =
    request.engine === 'webapi' && source.type === 'url' && source.value
      ? {
          songId: request.songId,
          quality: request.quality,
          url: source.value,
          ...(request.expectedBytes !== undefined ? { expectedBytes: request.expectedBytes } : {}),
          ...(request.durationMs !== undefined ? { durationMs: request.durationMs } : {})
        }
      : undefined

  return {
    engine: request.engine,
    source,
    metadataPath: source.metadataPath ?? '',
    initialBufferedPercent: source.type === 'file' ? 100 : 0,
    ...(backgroundCacheRequest ? { backgroundCacheRequest } : {})
  }
}

export function startPlaybackBackgroundCache(
  cache: PreparedPlaybackCache
): Promise<CachedSongSource> | null {
  const request = cache.backgroundCacheRequest
  if (!request) {
    return null
  }

  return cacheSongSource(
    request.songId,
    request.quality,
    request.url,
    request.expectedBytes,
    request.durationMs
  )
}
