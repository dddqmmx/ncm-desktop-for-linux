import { BrowserWindow, dialog, type OpenDialogOptions } from 'electron'
import { promises as fs } from 'fs'
import path from 'path'
import { NativeService } from './nativeService'

const AUDIO_EXTENSIONS = new Set([
  '.aac',
  '.aif',
  '.aiff',
  '.ape',
  '.flac',
  '.m4a',
  '.mp3',
  '.ogg',
  '.opus',
  '.wav',
  '.webm',
  '.wma'
])

const MAX_SCAN_DEPTH = 12
const MAX_SCAN_FILES = 50_000
const FILE_CHECK_BATCH_SIZE = 128
const DURATION_CHECK_BATCH_SIZE = 8

export interface ScannedAudioFile {
  filePath: string
  fileName: string
  rootPath: string
  duration: number
}

export interface LibraryScanResult {
  files: ScannedAudioFile[]
  scannedRoots: string[]
  truncated: boolean
}

export interface LibraryFileCheckResult {
  missingPaths: string[]
}

export interface LibraryFileDuration {
  filePath: string
  duration: number
}

function isAudioFile(fileName: string): boolean {
  return AUDIO_EXTENSIONS.has(path.extname(fileName).toLowerCase())
}

function isUsableWindow(window?: BrowserWindow | null): window is BrowserWindow {
  return !!window && !window.isDestroyed()
}

async function walkDirectory(
  rootPath: string,
  currentPath: string,
  depth: number,
  files: ScannedAudioFile[],
  state: { truncated: boolean }
): Promise<void> {
  if (state.truncated || depth > MAX_SCAN_DEPTH || files.length >= MAX_SCAN_FILES) {
    if (files.length >= MAX_SCAN_FILES) state.truncated = true
    return
  }

  let entries
  try {
    entries = await fs.readdir(currentPath, { withFileTypes: true })
  } catch {
    return
  }

  for (const entry of entries) {
    if (state.truncated || files.length >= MAX_SCAN_FILES) {
      state.truncated = true
      break
    }

    if (entry.name.startsWith('.')) continue

    const absolutePath = path.join(currentPath, entry.name)
    if (entry.isDirectory()) {
      await walkDirectory(rootPath, absolutePath, depth + 1, files, state)
      continue
    }

    if (!entry.isFile() || !isAudioFile(entry.name)) continue

    files.push({
      filePath: absolutePath,
      fileName: entry.name,
      rootPath,
      duration: 0
    })
  }
}

async function readFileDurations(paths: string[]): Promise<LibraryFileDuration[]> {
  const uniquePaths = [...new Set(paths.map((item) => item.trim()).filter(Boolean))]
  const durations: LibraryFileDuration[] = []

  for (let index = 0; index < uniquePaths.length; index += DURATION_CHECK_BATCH_SIZE) {
    const batch = uniquePaths.slice(index, index + DURATION_CHECK_BATCH_SIZE)
    const results = await Promise.all(
      batch.map(async (filePath): Promise<LibraryFileDuration | null> => {
        try {
          const duration = await NativeService.getFileDuration(filePath)
          if (!Number.isFinite(duration) || duration <= 0) return null
          return { filePath, duration: Math.round(duration) }
        } catch {
          return null
        }
      })
    )
    durations.push(...results.filter((item): item is LibraryFileDuration => item !== null))
  }

  return durations
}

export const LibraryService = {
  async selectFolder(parentWindow?: BrowserWindow | null): Promise<string | null> {
    const options: OpenDialogOptions = {
      title: '选择本地音乐文件夹',
      properties: ['openDirectory', 'createDirectory']
    }

    // 透明/无边框窗口做 parent 在 Linux 上容易导致原生对话框失败，优先无 parent。
    let result
    try {
      result = await dialog.showOpenDialog(options)
    } catch (error) {
      if (!isUsableWindow(parentWindow)) {
        throw error
      }
      result = await dialog.showOpenDialog(parentWindow, options)
    }

    if (result.canceled || result.filePaths.length === 0) {
      return null
    }

    return path.resolve(result.filePaths[0])
  },

  async scanFolders(paths: string[]): Promise<LibraryScanResult> {
    const files: ScannedAudioFile[] = []
    const scannedRoots: string[] = []
    const state = { truncated: false }
    const uniqueRoots = [
      ...new Set(
        paths
          .map((item) => item.trim())
          .filter(Boolean)
          .map((item) => path.resolve(item))
      )
    ]

    for (const rootPath of uniqueRoots) {
      let stats
      try {
        stats = await fs.stat(rootPath)
      } catch {
        continue
      }

      if (!stats.isDirectory()) continue

      scannedRoots.push(rootPath)
      await walkDirectory(rootPath, rootPath, 0, files, state)
      if (state.truncated) break
    }

    const durations = new Map(
      (await readFileDurations(files.map((file) => file.filePath))).map((item) => [
        item.filePath,
        item.duration
      ])
    )
    for (const file of files) {
      file.duration = durations.get(file.filePath) ?? 0
    }

    files.sort((left, right) => left.filePath.localeCompare(right.filePath, 'zh-CN'))
    return { files, scannedRoots, truncated: state.truncated }
  },

  getFileDurations(paths: string[]): Promise<LibraryFileDuration[]> {
    return readFileDurations(paths)
  },

  async checkFiles(paths: string[]): Promise<LibraryFileCheckResult> {
    const uniquePaths = [...new Set(paths.map((item) => item.trim()).filter(Boolean))]
    const missingPaths: string[] = []

    for (let index = 0; index < uniquePaths.length; index += FILE_CHECK_BATCH_SIZE) {
      const batch = uniquePaths.slice(index, index + FILE_CHECK_BATCH_SIZE)
      const results = await Promise.all(
        batch.map(async (filePath): Promise<string | null> => {
          try {
            const stats = await fs.stat(filePath)
            return stats.isFile() ? null : filePath
          } catch (error) {
            const code = (error as NodeJS.ErrnoException).code
            if (code === 'ENOENT' || code === 'ENOTDIR') return filePath

            // Keep the entry when existence cannot be determined, for example on a temporarily
            // unavailable mount or when permissions changed.
            return null
          }
        })
      )
      missingPaths.push(...results.filter((filePath): filePath is string => filePath !== null))
    }

    return { missingPaths }
  }
}
