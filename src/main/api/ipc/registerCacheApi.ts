import { ipcMain, type IpcMainInvokeEvent } from 'electron'
import { CacheService } from '../service/cacheService'

function handle(channel: string, fn: () => Promise<unknown>): void {
  ipcMain.handle(channel, () => fn())
}

function handleWithArg<T>(channel: string, fn: (arg: T) => Promise<unknown>): void {
  ipcMain.handle(channel, (_event: IpcMainInvokeEvent, arg: T) => fn(arg))
}

export function registerCacheApi(): void {
  handle('cache:getStats', () => CacheService.getStats())
  handleWithArg<number>('cache:setMaxSizeBytes', (v) => CacheService.setMaxSizeBytes(v))
  handle('cache:getSongMaxCacheAheadBytes', () => CacheService.getSongMaxCacheAheadBytes())
  handleWithArg<number>('cache:setSongMaxCacheAheadBytes', (v) =>
    CacheService.setSongMaxCacheAheadBytes(v)
  )
  handle('cache:clear', () => CacheService.clear())
  handleWithArg<string>('cache:resolveCachedMediaUrl', (v) => CacheService.resolveCachedMediaUrl(v))
  handleWithArg<{
    songId: number
    quality: string
    url: string
    expectedBytes?: number
  }>('cache:prepareSongSource', (payload) => CacheService.prepareSongSource(payload))
  handleWithArg<{
    songId: number
    quality: string
    url: string
    expectedBytes?: number
    durationMs?: number
  }>('cache:cacheSongSource', (payload) => CacheService.cacheSongSource(payload))
  handleWithArg<{
    metadataPath: string
    playbackPositionMs: number
  }>('cache:updateSongPlaybackPosition', (payload) =>
    CacheService.updateSongCachePlaybackPosition(payload)
  )
  handleWithArg<string>('cache:cancelSongDownload', (metadataPath) =>
    CacheService.cancelSongCacheDownload(metadataPath)
  )
  handleWithArg<string>('cache:getSongCacheProgress', (v) => CacheService.getSongCacheProgress(v))

  console.log('Cache API registered successfully.')
}
