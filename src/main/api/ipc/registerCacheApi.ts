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
      }
    ) => {
      void _event
      return CacheService.prepareSongSource(payload)
    }
  )

  console.log('Cache API registered successfully.')
}
