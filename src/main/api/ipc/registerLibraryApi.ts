import { BrowserWindow, ipcMain, type IpcMainInvokeEvent } from 'electron'
import { LibraryService } from '../service/libraryService'

export function registerLibraryApi(getParentWindow?: () => BrowserWindow | null): void {
  ipcMain.handle('library:selectFolder', (event: IpcMainInvokeEvent) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender)
    const parent = senderWindow ?? getParentWindow?.() ?? null
    return LibraryService.selectFolder(parent)
  })

  ipcMain.handle('library:scanFolders', (_event: IpcMainInvokeEvent, paths: string[]) => {
    const roots = Array.isArray(paths)
      ? paths.filter((item): item is string => typeof item === 'string')
      : []
    return LibraryService.scanFolders(roots)
  })

  ipcMain.handle('library:checkFiles', (_event: IpcMainInvokeEvent, paths: string[]) => {
    const filePaths = Array.isArray(paths)
      ? paths.filter((item): item is string => typeof item === 'string')
      : []
    return LibraryService.checkFiles(filePaths)
  })

  ipcMain.handle('library:getFileDurations', (_event: IpcMainInvokeEvent, paths: string[]) => {
    const filePaths = Array.isArray(paths)
      ? paths.filter((item): item is string => typeof item === 'string')
      : []
    return LibraryService.getFileDurations(filePaths)
  })
}
