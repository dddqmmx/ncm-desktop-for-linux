import { BrowserWindow, ipcMain } from 'electron'
import { UiService } from '../service/uiService'

export function registerUiApi(mainWindow: BrowserWindow): void {
  ipcMain.handle('ui:openSettingsWindow', () => {
    UiService.openSettingsWindow(mainWindow)
  })
  ipcMain.handle('ui:closeSettingsWindow', () => {
    UiService.closeSettingsWindow()
  })
  ipcMain.handle('ui:getAppInfo', () => {
    return UiService.getAppInfo()
  })
}
