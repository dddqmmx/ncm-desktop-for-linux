import { app, BrowserWindow } from 'electron'
import { join } from 'path'
import { is } from '@electron-toolkit/utils'

let settingsWindow: BrowserWindow | null = null

export const UiService = {
  openSettingsWindow(mainWindow:BrowserWindow) {
    if (settingsWindow) {
      settingsWindow.focus()
      return
    }

    settingsWindow = new BrowserWindow({
      width: 1000,
      height: 800,
      minWidth: 1000,
      minHeight: 800,
      opacity: 0,
      transparent: true,
      show: false,
      frame: false,
      autoHideMenuBar: true,
      parent: mainWindow,
      webPreferences: {
        preload: join(__dirname, '../preload/index.js'),
        sandbox: false
      }
    })

    settingsWindow.on('closed', () => {
      settingsWindow = null
    })

    settingsWindow.on('ready-to-show', () => {
      settingsWindow!.show()
    })

    if (is.dev && process.env['ELECTRON_RENDERER_URL']) {
      settingsWindow.loadURL(`${process.env['ELECTRON_RENDERER_URL']}#/settings`)
    } else {
      settingsWindow.loadFile(join(__dirname, '../renderer/index.html'), { hash: '/settings' })
    }
  },

  closeSettingsWindow() {
    settingsWindow?.close()
  },

  getAppInfo() {
    return {
      name: app.getName(),
      version: app.getVersion()
    }
  }
}
