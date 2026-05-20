import { BrowserWindow, ipcMain } from 'electron'
import { DialogWindowOptions, UiService } from '../service/uiService'

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
  ipcMain.handle('ui:openDialogWindow', (_event, options: DialogWindowOptions) => {
    return UiService.openDialogWindow(options)
  })
}
