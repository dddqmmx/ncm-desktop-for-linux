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
  // 主窗口的窗口底色跟随主题：页面本身透明，窗口底色会从边缘亚像素缝里露出
  // 一条线（默认白）。只作用于主窗口——设置/对话框窗口是真透明窗口，给它们
  // 上不透明底色会破坏圆角外的大片透明区域。
  ipcMain.handle('ui:setWindowBackground', (event, color: string) => {
    if (!/^#[0-9a-fA-F]{6}$/.test(color)) return
    if (BrowserWindow.fromWebContents(event.sender) !== mainWindow) return
    mainWindow.setBackgroundColor(color)
  })
}
