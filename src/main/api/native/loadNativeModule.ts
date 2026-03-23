import fs from 'fs'
import path from 'path'
import { app } from 'electron'

export type NativeCacheStats = {
  totalBytes?: number
  total_bytes?: number
  maxSizeBytes?: number
  max_size_bytes?: number
  songBytes?: number
  song_bytes?: number
  songEntries?: number
  song_entries?: number
  entityBytes?: number
  entity_bytes?: number
  entityEntries?: number
  entity_entries?: number
  coverBytes?: number
  cover_bytes?: number
  coverEntries?: number
  cover_entries?: number
  lyricBytes?: number
  lyric_bytes?: number
  lyricEntries?: number
  lyric_entries?: number
}

export interface NativePlayerBinding {
  playUrl(url: string, startSecs?: number): void
  playUrlCached(
    url: string,
    cachePath: string,
    metadataPath: string,
    durationMs?: number,
    cacheAheadSecs?: number,
    startSecs?: number
  ): void
  playFile(filePath: string, startSecs?: number): void
  pause(): void
  resume(): void
  stop(): void
  readonly progressMs: number
  readonly isPlaying: boolean
  seek(time: number): void
  switchOutputDevice(deviceId?: string): Promise<void>
  getOutputDevices(): Promise<unknown[]>
  waitFinished(): Promise<void>
}

export interface NativeCacheBinding {
  getStats(): NativeCacheStats
  getJson(bucket: string, key: string): string | null | undefined
  putJson(bucket: string, key: string, value: string): NativeCacheStats
  setMaxSizeBytes(maxSizeBytes: number): NativeCacheStats
  getSongCacheAheadSecs(): number
  setSongCacheAheadSecs(songCacheAheadSecs: number): number
  clear(): NativeCacheStats
  cacheRemoteFile(bucket: string, key: string, url: string): Promise<string | null | undefined>
  prepareSongSource(
    songId: number,
    quality: string,
    url: string
  ): {
    type?: string
    value?: string
    cachePath?: string
    cache_path?: string
    metadataPath?: string
    metadata_path?: string
    cacheAheadSecs?: number
    cache_ahead_secs?: number
  }
}

type NativeModule = {
  PlayerService: new () => NativePlayerBinding
  CacheService: new (rootDir: string, maxSizeBytes?: number) => NativeCacheBinding
}

let nativeModule: NativeModule | null = null

function resolveNative(): string {
  if (!app.isPackaged) {
    return path.join(__dirname, '..', '..', 'native', 'index.node')
  }

  const appPath = app.getAppPath()

  if (!appPath.endsWith('.asar')) {
    return path.join(appPath, 'native', 'index.node')
  }

  const asarDir = path.dirname(appPath)
  const asarNative = path.join(asarDir, 'native', 'index.node')
  if (fs.existsSync(asarNative)) {
    return asarNative
  }

  return path.join(process.resourcesPath, 'native', 'index.node')
}

export function getNativeModule(): NativeModule {
  if (!nativeModule) {
    // eslint-disable-next-line @typescript-eslint/no-require-imports -- native .node must be loaded via require
    nativeModule = require(resolveNative()) as NativeModule
  }

  return nativeModule
}
