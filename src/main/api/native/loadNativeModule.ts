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
  playUrl(
    url: string,
    startSecs?: number,
    strictBitPerfect?: boolean,
    exclusive?: boolean
  ): Promise<void>
  playUrlCached(
    url: string,
    cachePath: string,
    metadataPath: string,
    durationMs?: number,
    cacheAheadSecs?: number,
    maxCacheAheadBytes?: number,
    startSecs?: number,
    strictBitPerfect?: boolean,
    exclusive?: boolean
  ): Promise<void>
  playFile(
    filePath: string,
    startSecs?: number,
    strictBitPerfect?: boolean,
    exclusive?: boolean
  ): Promise<void>
  getFileDurationMs(filePath: string): Promise<number>
  pause(): void
  resume(): void
  stop(): void
  readonly progressMs: number
  readonly isPlaying: boolean
  readonly isBuffering: boolean
  seek(time: number): void
  /** 应用级软件音量（0.0~1.0），仅影响本应用输出 */
  setVolume(volume: number): void
  /**
   * 切换输出端点。后两个参数是调用方此刻的播放选项：会话里那份是开播时冻结的，
   * 切到不支持独占的端点（系统默认输出是虚拟端点）时照搬会让重放被拒绝。
   * 均不传则沿用会话原有选项。
   */
  switchOutputDevice(
    deviceId?: string,
    strictBitPerfect?: boolean,
    exclusive?: boolean
  ): Promise<void>
  getOutputDevices(): Promise<unknown[]>
  /** 阻塞到当前播放会话结束，返回结束原因。 */
  waitFinished(): Promise<NativeFinishReason>
}

/** 播放会话结束原因（对应 Rust FinishReason::as_str）。 */
export type NativeFinishReason = 'ended' | 'stopped' | 'replaced' | 'error'

export interface NativeSongCacheProgress {
  downloadedBytes: number
  totalBytes: number
  percent: number
  isComplete: boolean
}

export interface NativeCacheBinding {
  getStats(): Promise<NativeCacheStats>
  getJson(bucket: string, key: string): Promise<string | null | undefined>
  putJson(bucket: string, key: string, value: string): Promise<NativeCacheStats>
  setMaxSizeBytes(maxSizeBytes: number): Promise<NativeCacheStats>
  getSongMaxCacheAheadBytes(): Promise<number>
  setSongMaxCacheAheadBytes(songMaxCacheAheadBytes: number): Promise<number>
  clear(): Promise<NativeCacheStats>
  cacheRemoteFile(bucket: string, key: string, url: string): Promise<string | null | undefined>
  prepareSongSource(
    songId: number,
    quality: string,
    url: string,
    expectedBytes?: number
  ): Promise<{
    type?: string
    value?: string
    cachePath?: string
    cache_path?: string
    metadataPath?: string
    metadata_path?: string
    cacheAheadSecs?: number
    cache_ahead_secs?: number
    maxCacheAheadBytes?: number
    max_cache_ahead_bytes?: number
  }>
  cacheSongSource(
    songId: number,
    quality: string,
    url: string,
    expectedBytes?: number,
    durationMs?: number
  ): Promise<{
    type?: string
    value?: string
    cachePath?: string
    cache_path?: string
    metadataPath?: string
    metadata_path?: string
    cacheAheadSecs?: number
    cache_ahead_secs?: number
    maxCacheAheadBytes?: number
    max_cache_ahead_bytes?: number
  }>
  updateSongCachePlaybackPosition(metadataPath: string, playbackPositionMs: number): boolean
  cancelSongCacheDownload(metadataPath: string): Promise<boolean>
  getSongCacheProgress(metadataPath: string): Promise<NativeSongCacheProgress>
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
