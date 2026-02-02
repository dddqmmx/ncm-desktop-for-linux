import { ipcMain } from "electron"
import { NativeService } from "../service/nativeService"

export function registerNativeApi(): void {
  ipcMain.handle('player:playUrl',  (_, url:string, startSecs?: number) => {
    return NativeService.playUrl(url, startSecs)
  })
  ipcMain.handle('player:playFile',  (_, filePath:string, startSecs?: number) => {
    return NativeService.playFile(filePath, startSecs)
  })
  ipcMain.handle('player:pause',  (_) => {
    return NativeService.pause()
  })
  ipcMain.handle('player:resume',  (_) => {
    return NativeService.resume()
  })
  ipcMain.handle('player:stop',  (_) => {
    return NativeService.stop()
  })
  ipcMain.handle('player:getProgress',  (_) => {
    return NativeService.getProgress()
  })
  ipcMain.handle('player:seek',  (_,time) => {
    return NativeService.seek(time)
  })
  ipcMain.handle('player:waitFinished', () => {
    return NativeService.waitFinished()
  })
  ipcMain.handle('player:playUrlAndWait', (_e, url: string, startSecs?: number) => {
    return NativeService.playUrlAndWait(url, startSecs)
  })
  console.log('Native API registered successfully.')
}
