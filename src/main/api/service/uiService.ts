import { app, BrowserWindow, ipcMain } from 'electron'
import { join } from 'path'
import { is } from '@electron-toolkit/utils'

let settingsWindow: BrowserWindow | null = null
let dialogWindow: BrowserWindow | null = null

export interface DialogWindowOptions {
  title: string
  message: string
  mode?: 'confirm' | 'confirm-cancel'
  confirmText?: string
  cancelText?: string
}

const escapeHtml = (value: string): string =>
  value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')

const buildDialogHtml = (id: string, options: Required<DialogWindowOptions>): string => {
  const title = escapeHtml(options.title)
  const message = escapeHtml(options.message)
  const confirmText = escapeHtml(options.confirmText)
  const cancelText = escapeHtml(options.cancelText)
  const showCancel = options.mode === 'confirm-cancel'

  return `<!doctype html>
<html>
<head>
  <meta charset="UTF-8" />
  <style>
    * { box-sizing: border-box; }
    html, body {
      width: 100%;
      height: 100%;
      margin: 0;
      overflow: hidden;
      color: #111827;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: transparent;
    }
    body {
      display: grid;
      place-items: center;
      padding: 18px;
    }
    main {
      width: 100%;
      min-height: 184px;
      display: flex;
      flex-direction: column;
      gap: 14px;
      padding: 22px;
      border: 1px solid rgba(148, 163, 184, 0.36);
      border-radius: 20px;
      background: rgba(255, 255, 255, 0.92);
      box-shadow: 0 22px 58px rgba(15, 23, 42, 0.22);
      backdrop-filter: blur(26px);
    }
    h1 {
      margin: 0;
      font-size: 18px;
      line-height: 1.35;
      font-weight: 750;
      letter-spacing: 0;
    }
    p {
      flex: 1;
      margin: 0;
      color: #4b5563;
      font-size: 14px;
      line-height: 1.65;
      white-space: pre-wrap;
      word-break: break-word;
    }
    .actions {
      display: flex;
      justify-content: flex-end;
      gap: 10px;
    }
    button {
      min-width: 82px;
      height: 38px;
      border: 0;
      border-radius: 10px;
      padding: 0 16px;
      font-size: 14px;
      font-weight: 700;
      cursor: pointer;
    }
    .primary {
      color: #fff;
      background: #6366f1;
    }
    .secondary {
      color: #111827;
      background: rgba(17, 24, 39, 0.08);
    }
    @media (prefers-color-scheme: dark) {
      body { color: #f9fafb; }
      main {
        border-color: rgba(148, 163, 184, 0.26);
        background: rgba(24, 24, 27, 0.92);
        box-shadow: 0 22px 58px rgba(0, 0, 0, 0.42);
      }
      p { color: #cbd5e1; }
      .secondary {
        color: #f9fafb;
        background: rgba(255, 255, 255, 0.12);
      }
    }
  </style>
</head>
<body>
  <main>
    <h1>${title}</h1>
    <p>${message}</p>
    <div class="actions">
      ${showCancel ? `<button class="secondary" data-result="false">${cancelText}</button>` : ''}
      <button class="primary" data-result="true" autofocus>${confirmText}</button>
    </div>
  </main>
  <script>
    const { ipcRenderer } = require('electron')
    const closeWith = (confirmed) => ipcRenderer.send('ui:dialogResult:${id}', confirmed)
    document.querySelectorAll('button[data-result]').forEach((button) => {
      button.addEventListener('click', () => closeWith(button.dataset.result === 'true'))
    })
    window.addEventListener('keydown', (event) => {
      if (event.key === 'Escape') closeWith(false)
      if (event.key === 'Enter') closeWith(true)
    })
  </script>
</body>
</html>`
}

export const UiService = {
  openSettingsWindow(mainWindow: BrowserWindow) {
    if (settingsWindow) {
      settingsWindow.focus()
      return
    }

    settingsWindow = new BrowserWindow({
      width: 1000,
      height: 800,
      minWidth: 1000,
      minHeight: 800,
      opacity: 1,
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

  openDialogWindow(options: DialogWindowOptions): Promise<boolean> {
    const normalizedOptions: Required<DialogWindowOptions> = {
      title: options.title,
      message: options.message,
      mode: options.mode ?? 'confirm',
      confirmText: options.confirmText ?? '确定',
      cancelText: options.cancelText ?? '取消'
    }
    const id = `${Date.now()}-${Math.random().toString(16).slice(2)}`
    const resultChannel = `ui:dialogResult:${id}`

    dialogWindow?.close()
    dialogWindow = new BrowserWindow({
      width: 420,
      height: 240,
      minWidth: 360,
      minHeight: 220,
      show: false,
      frame: false,
      resizable: false,
      transparent: true,
      autoHideMenuBar: true,
      alwaysOnTop: true,
      webPreferences: {
        nodeIntegration: true,
        contextIsolation: false,
        sandbox: false
      }
    })

    return new Promise((resolve) => {
      let settled = false
      const finish = (confirmed: boolean): void => {
        if (settled) {
          return
        }

        settled = true
        ipcMain.removeAllListeners(resultChannel)
        resolve(confirmed)
        dialogWindow?.close()
      }

      ipcMain.once(resultChannel, (_event, confirmed: boolean) => {
        finish(confirmed)
      })

      dialogWindow?.on('closed', () => {
        dialogWindow = null
        finish(false)
      })

      dialogWindow?.on('ready-to-show', () => {
        dialogWindow?.show()
        dialogWindow?.focus()
      })

      dialogWindow?.loadURL(
        `data:text/html;charset=UTF-8,${encodeURIComponent(buildDialogHtml(id, normalizedOptions))}`
      )
    })
  },

  getAppInfo() {
    return {
      name: app.getName(),
      version: app.getVersion()
    }
  }
}
