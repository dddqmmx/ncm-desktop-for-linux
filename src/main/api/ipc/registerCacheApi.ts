import { ipcMain } from 'electron'
import { CacheService } from '../service/cacheService'

export function registerCacheApi(): void {
  ipcMain.handle('cache:getStats', () => {
    return CacheService.getStats()
  })

  ipcMain.handle('cache:setMaxSizeBytes', (_event, maxSizeBytes: number) => {
    void _event
    return CacheService.setMaxSizeBytes(maxSizeBytes)
  })

  ipcMain.handle('cache:getSongCacheAheadSecs', () => {
    return CacheService.getSongCacheAheadSecs()
  })

  ipcMain.handle('cache:setSongCacheAheadSecs', (_event, songCacheAheadSecs: number) => {
    void _event
    return CacheService.setSongCacheAheadSecs(songCacheAheadSecs)
  })

  ipcMain.handle('cache:getSongMaxCacheAheadBytes', () => {
    return CacheService.getSongMaxCacheAheadBytes()
  })

  ipcMain.handle('cache:setSongMaxCacheAheadBytes', (_event, songMaxCacheAheadBytes: number) => {
    void _event
    return CacheService.setSongMaxCacheAheadBytes(songMaxCacheAheadBytes)
  })

  ipcMain.handle('cache:clear', () => {
    return CacheService.clear()
  })

  ipcMain.handle('cache:resolveCachedMediaUrl', (_event, url: string) => {
    void _event
    return CacheService.resolveCachedMediaUrl(url)
  })

  ipcMain.handle(
    'cache:prepareSongSource',
    (
      _event,
      payload: {
        songId: number
        quality: string
        url: string
        expectedBytes?: number
      }
    ) => {
      void _event
      return CacheService.prepareSongSource(payload)
    }
  )

  ipcMain.handle('cache:getSongCacheProgress', (_event, metadataPath: string) => {
    void _event
    return CacheService.getSongCacheProgress(metadataPath)
  })

  console.log('Cache API registered successfully.')
}
