import { ipcMain } from 'electron'
import { UiService } from '../service/uiService'

export function registerUiApi(): void {
  ipcMain.handle('ui:openSettingsWindow', () => {
    UiService.openSettingsWindow()
  })
}
