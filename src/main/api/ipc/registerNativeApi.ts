import { ipcMain } from 'electron'
import { NativeService } from '../service/nativeService'

export function registerNativeApi(): void {
  ipcMain.handle('player:playUrl', (_event, url: string, startSecs?: number) => {
    void _event
    return NativeService.playUrl(url, startSecs)
  })
  ipcMain.handle(
    'player:playUrlCached',
    (
      _event,
      url: string,
      cachePath: string,
      metadataPath: string,
      durationMs?: number,
      cacheAheadSecs?: number,
      startSecs?: number
    ) => {
      void _event
      return NativeService.playUrlCached(
        url,
        cachePath,
        metadataPath,
        durationMs,
        cacheAheadSecs,
        startSecs
      )
    }
  )
  ipcMain.handle('player:playFile', (_event, filePath: string, startSecs?: number) => {
    void _event
    return NativeService.playFile(filePath, startSecs)
  })
  ipcMain.handle('player:pause', () => {
    return NativeService.pause()
  })
  ipcMain.handle('player:resume', () => {
    return NativeService.resume()
  })
  ipcMain.handle('player:stop', () => {
    return NativeService.stop()
  })
  ipcMain.handle('player:getProgress', () => {
    return NativeService.getProgress()
  })
  ipcMain.handle('player:seek', (_event, time: number) => {
    void _event
    return NativeService.seek(time)
  })
  ipcMain.handle('player:switchOutputDevice', (_event, deviceId?: string) => {
    void _event
    return NativeService.switchOutputDevice(deviceId)
  })
  ipcMain.handle('player:getOutputDevices', () => {
    return NativeService.getOutputDevices()
  })
  ipcMain.handle('player:waitFinished', () => {
    return NativeService.waitFinished()
  })
  ipcMain.handle('player:playUrlAndWait', (_e, url: string, startSecs?: number) => {
    void _e
    return NativeService.playUrlAndWait(url, startSecs)
  })
  console.log('Native API registered successfully.')
}
