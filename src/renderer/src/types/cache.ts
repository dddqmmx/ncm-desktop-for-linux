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
