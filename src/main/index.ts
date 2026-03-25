import { app, shell, BrowserWindow, ipcMain } from 'electron'
import { join } from 'path'
import { electronApp, optimizer, is } from '@electron-toolkit/utils'
import { registerMusicApi } from './api/ipc/registerMusicApi'
import { registerNativeApi } from './api/ipc/registerNativeApi'
import { registerCacheApi } from './api/ipc/registerCacheApi'
import { registerUiApi } from './api/ipc/registerUiApi'
import { registerCacheProtocol } from './api/protocol/registerCacheProtocol'

const iconPath = join(__dirname, '../../resources/icon.png')

let mainWindow: BrowserWindow | null = null

// 将函数改为返回 BrowserWindow 实例
function createWindow(): BrowserWindow {
  // 使用局部变量 win 进行配置，完美解决 TS 在回调函数中的 null 警告
  const win = new BrowserWindow({
    width: 1435,
    height: 1000,
    minWidth: 1340,
    minHeight: 960,
    show: false,
    frame: false,
    autoHideMenuBar: true,
    ...(process.platform === 'linux' ? { icon: iconPath } : {}),
    webPreferences: {
      preload: join(__dirname, '../preload/index.js'),
      sandbox: false
    }
  })

  win.on('ready-to-show', () => {
    win.show()
  })

  win.webContents.setWindowOpenHandler((details) => {
    shell.openExternal(details.url)
    return { action: 'deny' }
  })

  // HMR for renderer base on electron-vite cli.
  // Load the remote URL for development or the local html file for production.
  if (is.dev && process.env['ELECTRON_RENDERER_URL']) {
    win.loadURL(process.env['ELECTRON_RENDERER_URL'])
  } else {
    win.loadFile(join(__dirname, '../renderer/index.html'))
  }

  return win
}

// This method will be called when Electron has finished
// initialization and is ready to create browser windows.
// Some APIs can only be used after this event occurs.
app.whenReady().then(() => {
  registerCacheProtocol(join(app.getPath('userData'), 'cache'))
  registerMusicApi()
  registerNativeApi()
  registerCacheApi()

  // 必须先创建窗口并赋值，再注册需要依赖窗口的 API
  mainWindow = createWindow()
  registerUiApi(mainWindow)

  // Set app user model id for windows
  electronApp.setAppUserModelId('com.electron')

  // Default open or close DevTools by F12 in development
  // and ignore CommandOrControl + R in production.
  // see https://github.com/alex8088/electron-toolkit/tree/master/packages/utils
  app.on('browser-window-created', (_, window) => {
    optimizer.watchWindowShortcuts(window)
  })

  // IPC test
  ipcMain.on('ping', () => console.log('pong'))

  app.on('activate', function () {
    // On macOS it's common to re-create a window in the app when the
    // dock icon is clicked and there are no other windows open.
    if (BrowserWindow.getAllWindows().length === 0) {
      mainWindow = createWindow()
      // 注意：如果在 MacOS 下窗口被关闭后重新创建，如果你之前的 registerUiApi 绑定的是旧窗口实例，
      // 可能需要在这里重新调用 registerUiApi(mainWindow) 来绑定新窗口。
    }
  })
})

// Quit when all windows are closed, except on macOS. There, it's common
// for applications and their menu bar to stay active until the user quits
// explicitly with Cmd + Q.
app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  }
})

// In this file you can include the rest of your app's specific main process
// code. You can also put them in separate files and require them here.
