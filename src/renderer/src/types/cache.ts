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
  maxCacheAheadBytes?: number
}

export type PlaybackCacheEngine = 'native' | 'webapi'

export interface PlaybackCacheRequest {
  engine: PlaybackCacheEngine
  songId: number
  quality: string
  url: string
  expectedBytes?: number
  durationMs?: number
}

export interface BackgroundPlaybackCacheRequest {
  songId: number
  quality: string
  url: string
  expectedBytes?: number
  durationMs?: number
}

export interface PreparedPlaybackCache {
  engine: PlaybackCacheEngine
  source: CachedSongSource
  metadataPath: string
  initialBufferedPercent: number
  backgroundCacheRequest?: BackgroundPlaybackCacheRequest
}

export interface SongCacheProgress {
  downloadedBytes: number
  totalBytes: number
  percent: number
  isComplete: boolean
}
