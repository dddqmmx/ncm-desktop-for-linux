import { BrowserWindow } from 'electron'
import { join } from 'path'
import { is } from '@electron-toolkit/utils'

export const UiService = {
  openSettingsWindow() {
    const settingsWindow = new BrowserWindow({
      width: 950,
      height: 800,
      minWidth: 950,
      minHeight: 800,
      opacity: 0,
      transparent: true,
      show: false,
      frame: false,
      autoHideMenuBar: true,
      webPreferences: {
        preload: join(__dirname, '../preload/index.js'),
        sandbox: false
      }
    })

    settingsWindow.on('ready-to-show', () => {
      settingsWindow.show()
    })

    if (is.dev && process.env['ELECTRON_RENDERER_URL']) {
      settingsWindow.loadURL(`${process.env['ELECTRON_RENDERER_URL']}#/settings`)
    } else {
      settingsWindow.loadFile(join(__dirname, '../renderer/index.html'), { hash: '/settings' })
    }
  }
}
