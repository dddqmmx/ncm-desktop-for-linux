import type { CachedSongSource } from '@renderer/types/cache'

function normalizeUrl(value: string | null | undefined): string {
  return typeof value === 'string' ? value.trim() : ''
}

export function clearResolvedMediaUrlCache(): void {
  // renderer 不再保留长期路径映射，避免淘汰后出现悬空 file URL
}

export async function resolveCachedMediaUrl(url: string | null | undefined): Promise<string> {
  const normalizedUrl = normalizeUrl(url)
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

  try {
    const resolvedUrl = await window.api.resolve_cached_media_url(normalizedUrl)
    return normalizeUrl(resolvedUrl) || normalizedUrl
  } catch (error) {
    console.warn('解析缓存媒体地址失败', error)
    return normalizedUrl
  }
}

export async function prepareCachedSongSource(
  songId: number,
  quality: string,
  url: string
): Promise<CachedSongSource> {
  const normalizedUrl = normalizeUrl(url)
  if (!normalizedUrl) {
    return {
      type: 'url',
      value: ''
    }
  }

  try {
    const payload = await window.api.prepare_cached_song_source({
      songId,
      quality,
      url: normalizedUrl
    })

    if (
      payload &&
      typeof payload === 'object' &&
      (payload.type === 'file' || payload.type === 'url') &&
      typeof payload.value === 'string'
    ) {
      return {
        type: payload.type,
        value: payload.value,
        ...(typeof payload.cachePath === 'string' && payload.cachePath.trim()
          ? { cachePath: payload.cachePath.trim() }
          : {}),
        ...(typeof payload.metadataPath === 'string' && payload.metadataPath.trim()
          ? { metadataPath: payload.metadataPath.trim() }
          : {}),
        ...(typeof payload.cacheAheadSecs === 'number' && Number.isFinite(payload.cacheAheadSecs)
          ? { cacheAheadSecs: payload.cacheAheadSecs }
          : {})
      }
    }
  } catch (error) {
    console.warn('准备缓存歌曲源失败', error)
  }

  return {
    type: 'url',
    value: normalizedUrl
  }
}
